//! an abstraction over the kinds of connections useful for irc clients

use pin_project_lite::pin_project;
use std::{
    fmt,
    net::SocketAddr,
    path::Path,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpStream, UnixStream},
};
use tokio_rustls::{
    client::TlsStream,
    rustls::{
        client::WebPkiServerVerifier,
        pki_types::{CertificateDer, PrivateKeyDer, ServerName},
        ClientConfig, RootCertStore,
    },
    TlsConnector,
};
use tokio_socks::{
    tcp::{socks4::Socks4Stream, socks5::Socks5Stream},
    IntoTargetAddr, TargetAddr,
};

pub use tokio_rustls;

mod danger;

/// error type returned by `irc_connect`
#[derive(Debug, foxerror::FoxError)]
#[non_exhaustive]
pub enum Error {
    /// you specified a tls client cert without using tls
    ClientCertNoTls,
    /// failed to connect
    #[err(from)]
    Connect(std::io::Error),
    /// could not sock
    #[err(from)]
    Socks(tokio_socks::Error),
    /// could not rustls
    #[err(from)]
    Rustls(tokio_rustls::rustls::Error),
}

pin_project! {
    /// an open connection
    #[derive(Debug)]
    pub struct Stream {
        #[pin]
        inner: MaybeTls,
    }
}

impl Stream {
    /// start building a new stream based on a tcp connection
    ///
    /// ```no_run
    /// use irc_connect::Stream;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let stream = Stream::new_tcp(&"[::1]:6667".parse().unwrap()).connect().await.unwrap();
    /// # }
    /// ```
    pub fn new_tcp(addr: &SocketAddr) -> StreamBuilder<'_> {
        StreamBuilder::new(BaseParams::Tcp(addr))
    }
    /// start building a new stream based on a unix socket
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use irc_connect::Stream;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let stream = Stream::new_unix(Path::new("./my-unix-socket")).connect().await.unwrap();
    /// # }
    /// ```
    pub fn new_unix(path: &Path) -> StreamBuilder<'_> {
        StreamBuilder::new(BaseParams::Unix(path))
    }
}

impl AsyncRead for Stream {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_read(cx, buf)
    }
}

impl AsyncWrite for Stream {
    #[inline]
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        self.project().inner.poll_write(cx, buf)
    }
    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        self.project().inner.poll_flush(cx)
    }
    #[inline]
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.project().inner.poll_shutdown(cx)
    }
}

pin_project! {
    #[project = MaybeTlsProj]
    #[derive(Debug)]
    #[allow(clippy::large_enum_variant)] // you should use tls most of the time
    enum MaybeTls {
        Plain {
            #[pin]
            inner: MaybeSocks,
        },
        Tls {
            #[pin]
            inner: TlsStream<MaybeSocks>,
        },
    }
}

macro_rules! trivial_impl {
    ($target:ty, ($($arm:path),*)) => {
        impl AsyncRead for $target {
            #[inline]
            fn poll_read(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &mut ReadBuf<'_>,
            ) -> Poll<std::io::Result<()>> {
                match self.project() {
                    $($arm { inner } => inner.poll_read(cx, buf),)*
                }
            }
        }

        impl AsyncWrite for $target {
            #[inline]
            fn poll_write(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<Result<usize, std::io::Error>> {
                match self.project() {
                    $($arm { inner } => inner.poll_write(cx, buf),)*
                }
            }
            #[inline]
            fn poll_flush(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
            ) -> Poll<Result<(), std::io::Error>> {
                match self.project() {
                    $($arm { inner } => inner.poll_flush(cx),)*
                }
            }
            #[inline]
            fn poll_shutdown(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
            ) -> Poll<Result<(), std::io::Error>> {
                match self.project() {
                    $($arm { inner } => inner.poll_shutdown(cx),)*
                }
            }
        }
    };
}

trivial_impl!(MaybeTls, (MaybeTlsProj::Plain, MaybeTlsProj::Tls));

pin_project! {
    #[project = MaybeSocksProj]
    #[derive(Debug)]
    enum MaybeSocks {
        Clear {
            #[pin]
            inner: BaseStream,
        },
        Socks4 {
            #[pin]
            inner: Socks4Stream<BaseStream>,
        },
        Socks5 {
            #[pin]
            inner: Socks5Stream<BaseStream>,
        },
    }
}

trivial_impl!(
    MaybeSocks,
    (
        MaybeSocksProj::Clear,
        MaybeSocksProj::Socks4,
        MaybeSocksProj::Socks5
    )
);

pin_project! {
    #[project = BaseStreamProj]
    #[derive(Debug)]
    enum BaseStream {
        Tcp {
            #[pin]
            inner: TcpStream,
        },
        Unix {
            #[pin]
            inner: UnixStream,
        },
    }
}

trivial_impl!(BaseStream, (BaseStreamProj::Tcp, BaseStreamProj::Unix));

/// a builder for [`Stream`]
#[derive(Debug)]
pub struct StreamBuilder<'a> {
    base: BaseParams<'a>,
    socks: Option<SocksParams<'a>>,
    tls: Option<TlsParams>,
    client_cert: Option<ClientCert>,
}

impl<'a> StreamBuilder<'a> {
    fn new(base: BaseParams<'a>) -> Self {
        Self {
            base,
            socks: None,
            tls: None,
            client_cert: None,
        }
    }

    fn socks(
        mut self,
        version: SocksVersion,
        target: impl IntoTargetAddr<'a>,
        auth: Option<SocksAuth<'a>>,
    ) -> Self {
        self.socks = Some(SocksParams {
            version,
            target: target.into_target_addr(),
            auth,
        });
        self
    }

    /// enable socks4 proxying
    ///
    /// ```
    /// # use irc_connect::Stream;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let addr = "[::1]:6667".parse().unwrap();
    /// # let builder = Stream::new_tcp(&addr);
    /// let builder = builder.socks4("127.0.0.1:9050");
    /// # }
    /// ```
    pub fn socks4(self, target: impl IntoTargetAddr<'a>) -> Self {
        self.socks(SocksVersion::Socks4, target, None)
    }

    /// enable socks4 proxying with a userid
    ///
    /// ```
    /// # use irc_connect::Stream;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let addr = "[::1]:6667".parse().unwrap();
    /// # let builder = Stream::new_tcp(&addr);
    /// let builder = builder.socks4_with_userid("127.0.0.1:9050", "meow");
    /// # }
    /// ```
    pub fn socks4_with_userid(self, target: impl IntoTargetAddr<'a>, userid: &'a str) -> Self {
        self.socks(
            SocksVersion::Socks4,
            target,
            Some(SocksAuth {
                username: userid,
                password: "h",
            }),
        )
    }

    /// enable socks5 proxying
    ///
    /// ```
    /// # use irc_connect::Stream;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let addr = "[::1]:6667".parse().unwrap();
    /// # let builder = Stream::new_tcp(&addr);
    /// let builder = builder.socks5("127.0.0.1:9050");
    /// # }
    /// ```
    pub fn socks5(self, target: impl IntoTargetAddr<'a>) -> Self {
        self.socks(SocksVersion::Socks5, target, None)
    }

    /// enable socks5 proxying with password authentication
    ///
    /// ```
    /// # use irc_connect::Stream;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let addr = "[::1]:6667".parse().unwrap();
    /// # let builder = Stream::new_tcp(&addr);
    /// let builder = builder.socks5_with_password("127.0.0.1:9050", "AzureDiamond", "hunter2");
    /// # }
    /// ```
    pub fn socks5_with_password(
        self,
        target: impl IntoTargetAddr<'a>,
        username: &'a str,
        password: &'a str,
    ) -> Self {
        self.socks(
            SocksVersion::Socks5,
            target,
            Some(SocksAuth { username, password }),
        )
    }

    fn tls(mut self, domain: ServerName<'static>, verification: TlsVerify) -> Self {
        self.tls = Some(TlsParams {
            domain,
            verification,
        });
        self
    }

    /// enable tls without any verification
    ///
    /// ```
    /// use tokio_rustls::rustls::pki_types::ServerName;
    /// # use irc_connect::Stream;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let addr = "[::1]:6667".parse().unwrap();
    /// # let builder = Stream::new_tcp(&addr);
    /// let builder = builder.tls_danger_insecure(ServerName::try_from("google.com").unwrap());
    /// # }
    /// ```
    pub fn tls_danger_insecure(self, domain: ServerName<'static>) -> Self {
        self.tls(domain, TlsVerify::Insecure)
    }

    /// enable tls with root certificate verification
    ///
    /// can also be used to pin a self-signed cert as long as it has a `CA:FALSE` constraint
    ///
    /// ```
    /// use tokio_rustls::rustls::RootCertStore;
    /// use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
    ///
    /// # use irc_connect::Stream;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let addr = "[::1]:6667".parse().unwrap();
    /// # let builder = Stream::new_tcp(&addr);
    /// let mut root = RootCertStore::empty();
    /// root.add_parsable_certificates(
    ///     CertificateDer::pem_file_iter("/etc/ssl/cert.pem")
    ///         .unwrap()
    ///         .flatten(),
    /// );
    /// let builder = builder.tls_with_root(ServerName::try_from("google.com").unwrap(), root);
    /// # }
    /// ```
    pub fn tls_with_root(
        self,
        domain: ServerName<'static>,
        root: impl Into<Arc<RootCertStore>>,
    ) -> Self {
        self.tls(domain, TlsVerify::CaStore(root.into()))
    }

    /// enable tls with a webpki verifier
    pub fn tls_with_webpki(
        self,
        domain: ServerName<'static>,
        webpki: Arc<WebPkiServerVerifier>,
    ) -> Self {
        self.tls(domain, TlsVerify::WebPki(webpki))
    }

    /// use a tls client certificate
    ///
    /// requires tls to be enabled
    ///
    /// ```no_run
    /// use irc_connect::Stream;
    /// use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let addr = "[::1]:6667".parse().unwrap();
    /// let builder = Stream::new_tcp(&addr).tls_danger_insecure(ServerName::from(addr.ip()));
    /// let cert = CertificateDer::pem_file_iter("cert.pem")
    ///     .unwrap()
    ///     .collect::<Result<Vec<_>, _>>()
    ///     .unwrap();
    /// let key = PrivateKeyDer::from_pem_file("cert.key").unwrap();
    /// let builder = builder.client_cert(cert, key);
    /// # }
    /// ```
    pub fn client_cert(
        mut self,
        cert_chain: Vec<CertificateDer<'static>>,
        key_der: PrivateKeyDer<'static>,
    ) -> Self {
        self.client_cert = Some(ClientCert {
            cert_chain,
            key_der,
        });
        self
    }

    /// finish building and open the connection
    ///
    /// ```no_run
    /// # use irc_connect::Stream;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let addr = "[::1]:6667".parse().unwrap();
    /// # let builder = Stream::new_tcp(&addr);
    /// let stream = builder.connect().await.unwrap();
    /// # }
    /// ```
    pub async fn connect(self) -> Result<Stream, Error> {
        let stream = match self.base {
            BaseParams::Tcp(addr) => BaseStream::Tcp {
                inner: TcpStream::connect(addr).await?,
            },
            BaseParams::Unix(path) => BaseStream::Unix {
                inner: UnixStream::connect(path).await?,
            },
        };
        let stream = if let Some(params) = self.socks {
            let target = params.target?;
            match params.version {
                SocksVersion::Socks4 => MaybeSocks::Socks4 {
                    inner: if let Some(SocksAuth { username, .. }) = params.auth {
                        Socks4Stream::connect_with_userid_and_socket(stream, target, username)
                            .await?
                    } else {
                        Socks4Stream::connect_with_socket(stream, target).await?
                    },
                },
                SocksVersion::Socks5 => MaybeSocks::Socks5 {
                    inner: if let Some(SocksAuth { username, password }) = params.auth {
                        Socks5Stream::connect_with_password_and_socket(
                            stream, target, username, password,
                        )
                        .await?
                    } else {
                        Socks5Stream::connect_with_socket(stream, target).await?
                    },
                },
            }
        } else {
            MaybeSocks::Clear { inner: stream }
        };
        let stream = if let Some(params) = self.tls {
            let config = ClientConfig::builder();
            let config = match params.verification {
                TlsVerify::Insecure => {
                    let provider = config.crypto_provider().clone();
                    config
                        .dangerous()
                        .with_custom_certificate_verifier(danger::PhonyVerify::new(provider))
                }
                TlsVerify::CaStore(root) => config.with_root_certificates(root),
                TlsVerify::WebPki(webpki) => config.with_webpki_verifier(webpki),
            };
            let config = if let Some(ClientCert {
                cert_chain,
                key_der,
            }) = self.client_cert
            {
                config.with_client_auth_cert(cert_chain, key_der)?
            } else {
                config.with_no_client_auth()
            };
            let connector = TlsConnector::from(Arc::new(config));
            let inner = connector.connect(params.domain, stream).await?;
            MaybeTls::Tls { inner }
        } else {
            if self.client_cert.is_some() {
                return Err(Error::ClientCertNoTls);
            }
            MaybeTls::Plain { inner: stream }
        };
        Ok(Stream { inner: stream })
    }
}

#[derive(Debug)]
enum BaseParams<'a> {
    // we cannot use [`tokio::net::ToSocketAddrs`] because they dont expose it :(
    Tcp(&'a SocketAddr),
    Unix(&'a Path),
}

struct SocksParams<'a> {
    version: SocksVersion,
    target: tokio_socks::Result<TargetAddr<'a>>,
    auth: Option<SocksAuth<'a>>,
}

impl fmt::Debug for SocksParams<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.version, f)
    }
}

struct SocksAuth<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Debug)]
enum SocksVersion {
    Socks4,
    Socks5,
}

#[derive(Debug)]
struct TlsParams {
    domain: ServerName<'static>,
    verification: TlsVerify,
}

#[derive(Debug)]
enum TlsVerify {
    Insecure,
    CaStore(Arc<RootCertStore>),
    WebPki(Arc<WebPkiServerVerifier>),
}

#[derive(Debug)]
struct ClientCert {
    cert_chain: Vec<CertificateDer<'static>>,
    key_der: PrivateKeyDer<'static>,
}
