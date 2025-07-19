use std::{fmt, net::SocketAddr, path::Path, sync::Arc};
use tokio::net::{TcpStream, UnixStream};
use tokio_rustls::{
    client::TlsStream,
    rustls::{client::WebPkiServerVerifier, pki_types::ServerName, RootCertStore},
};
use tokio_socks::{
    tcp::{socks4::Socks4Stream, socks5::Socks5Stream},
    IntoTargetAddr,
};

pub use tokio_rustls;

#[derive(Debug)]
pub struct Stream {
    inner: MaybeTls,
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
}

#[derive(Debug)]
enum BaseParams<'a> {
    // we cannot use [`tokio::net::ToSocketAddrs`] because they dont expose it :(
    Tcp(&'a SocketAddr),
    Unix(&'a Path),
}

struct SocksParams<'a> {
    version: SocksVersion,
    target: Box<dyn IntoTargetAddr<'a>>,
}

impl fmt::Debug for SocksParams<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.version, f)
    }
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
