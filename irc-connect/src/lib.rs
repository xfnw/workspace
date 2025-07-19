use std::{net::SocketAddr, path::Path, sync::Arc};
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

pub struct Stream {
    inner: MaybeTls,
}

#[allow(clippy::large_enum_variant)] // you should use tls most of the time
enum MaybeTls {
    Plain(MaybeSocks),
    Tls(TlsStream<MaybeSocks>),
}

enum MaybeSocks {
    Clear(BaseStream),
    Socks4(Socks4Stream<BaseStream>),
    Socks5(Socks5Stream<BaseStream>),
}

enum BaseStream {
    Tcp(TcpStream),
    Unix(UnixStream),
}

pub struct StreamBuilder<'a> {
    base: BaseParams<'a>,
    socks: Option<SocksParams<'a>>,
    tls: Option<TlsParams<'a>>,
}

enum BaseParams<'a> {
    // we cannot use [`tokio::net::ToSocketAddrs`] because they dont expose it :(
    Tcp(&'a SocketAddr),
    Unix(&'a Path),
}

struct SocksParams<'a> {
    version: SocksVersion,
    target: Box<dyn IntoTargetAddr<'a>>,
}

enum SocksVersion {
    Socks4,
    Socks5,
}

struct TlsParams<'a> {
    domain: ServerName<'a>,
    verification: TlsVerify,
}

enum TlsVerify {
    Insecure,
    CaStore(Arc<RootCertStore>),
    WebPki(Arc<WebPkiServerVerifier>),
}
