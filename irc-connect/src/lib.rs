use std::{fmt, net::SocketAddr, path::Path, sync::Arc};
use tokio::net::{TcpStream, UnixStream};
use tokio_rustls::{
    client::TlsStream,
    rustls::{
        client::WebPkiServerVerifier,
        pki_types::{CertificateDer, PrivateKeyDer, ServerName},
        RootCertStore,
    },
};
use tokio_socks::{
    tcp::{socks4::Socks4Stream, socks5::Socks5Stream},
    IntoTargetAddr, TargetAddr,
};

pub use tokio_rustls;

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
}

#[derive(Debug)]
pub struct Stream {
    inner: MaybeTls,
}

impl Stream {
    pub fn new_tcp(addr: &SocketAddr) -> StreamBuilder<'_> {
        StreamBuilder::new(BaseParams::Tcp(addr))
    }
    pub fn new_unix(path: &Path) -> StreamBuilder<'_> {
        StreamBuilder::new(BaseParams::Unix(path))
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // you should use tls most of the time
enum MaybeTls {
    Plain(MaybeSocks),
    Tls(TlsStream<MaybeSocks>),
}

#[derive(Debug)]
enum MaybeSocks {
    Clear(BaseStream),
    Socks4(Socks4Stream<BaseStream>),
    Socks5(Socks5Stream<BaseStream>),
}

#[derive(Debug)]
enum BaseStream {
    Tcp(TcpStream),
    Unix(UnixStream),
}

#[derive(Debug)]
pub struct StreamBuilder<'a> {
    base: BaseParams<'a>,
    socks: Option<SocksParams<'a>>,
    tls: Option<TlsParams<'a>>,
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

    pub fn socks4(self, target: impl IntoTargetAddr<'a>) -> Self {
        self.socks(SocksVersion::Socks4, target, None)
    }

    pub fn socks4_with_password(
        self,
        target: impl IntoTargetAddr<'a>,
        username: &'a str,
        password: &'a str,
    ) -> Self {
        self.socks(
            SocksVersion::Socks4,
            target,
            Some(SocksAuth { username, password }),
        )
    }

    pub fn socks5(mut self, target: impl IntoTargetAddr<'a>) -> Self {
        self.socks(SocksVersion::Socks5, target, None)
    }

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

    fn tls(mut self, domain: ServerName<'a>, verification: TlsVerify) -> Self {
        self.tls = Some(TlsParams {
            domain,
            verification,
        });
        self
    }

    pub fn tls_insecure(self, domain: ServerName<'a>) -> Self {
        self.tls(domain, TlsVerify::Insecure)
    }

    pub fn tls_with_root(
        self,
        domain: ServerName<'a>,
        root: impl Into<Arc<RootCertStore>>,
    ) -> Self {
        self.tls(domain, TlsVerify::CaStore(root.into()))
    }

    pub fn tls_with_webpki(
        self,
        domain: ServerName<'a>,
        webpki: Arc<WebPkiServerVerifier>,
    ) -> Self {
        self.tls(domain, TlsVerify::WebPki(webpki))
    }

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

    pub async fn build(self) -> Result<Stream, Error> {
        let stream = match self.base {
            BaseParams::Tcp(addr) => BaseStream::Tcp(TcpStream::connect(addr).await?),
            BaseParams::Unix(path) => BaseStream::Unix(UnixStream::connect(path).await?),
        };
        let stream = if let Some(params) = self.socks {
            let target = params.target?;
            match params.version {
                SocksVersion::Socks4 => MaybeSocks::Socks4(
                    if let Some(SocksAuth { username, password }) = params.auth {
                        todo!()
                    } else {
                        //Socks4Stream::connect_with_socket(stream, target).await?
                        todo!()
                    },
                ),
                SocksVersion::Socks5 => MaybeSocks::Socks5(todo!()),
            }
        } else {
            MaybeSocks::Clear(stream)
        };
        todo!()
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
struct TlsParams<'a> {
    domain: ServerName<'a>,
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
