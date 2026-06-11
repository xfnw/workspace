// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: EUPL-1.2

use argh::FromArgs;
use bytes::{Buf, Bytes};
use http_body_util::{BodyExt, Empty};
use hyper::{Request, Response, StatusCode, body::Incoming, header::HeaderValue};
use hyper_util::rt::TokioIo;
use irc_connect::{
    Connection,
    tokio_rustls::rustls::{
        RootCertStore,
        pki_types::{CertificateDer, ServerName, pem::PemObject},
    },
};
use irctokens::Line;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fmt,
    net::IpAddr,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex as AMutex, mpsc},
};
use url::Url;

/// copyparty to xdcc bridge
#[derive(Debug, FromArgs)]
struct Opt {
    /// milliseconds to wait between sending messages
    #[argh(option, default = "0")]
    delay: u64,
    /// seconds to wait between reconnection attempts
    #[argh(option)]
    autoconn: Option<u64>,
    /// channel to autojoin
    #[argh(option)]
    join: Option<String>,
    /// nickname to use
    #[argh(option, default = "\"offerparty\".to_string()")]
    nick: String,
    /// enable tls for connecting to irc
    #[argh(switch, short = 't')]
    tls: bool,
    /// path of certificate store to trust
    #[argh(option, default = "\"/etc/ssl/certs/ca-bundle.crt\".into()")]
    trust: PathBuf,
    /// authorization header value to send copyparty
    #[argh(option)]
    auth: Option<HeaderValue>,
    /// copyparty url to get files from
    #[argh(positional)]
    url: Url,
    /// irc server address to connect to
    #[argh(positional)]
    addr: String,
}

#[derive(Debug, foxerror::FoxError)]
enum Error {
    #[err(from)]
    Io(std::io::Error),
    Send,
    #[err(from)]
    Tokenise(irctokens::tokenise::Error),
    #[err(from)]
    ParseInt(std::num::ParseIntError),
    UnknownId,
    NotADirectory(String),
    #[err(from)]
    UrlParse(url::ParseError),
    #[err(from)]
    Http(hyper::http::Error),
    #[err(from)]
    Hyper(hyper::Error),
    #[err(from)]
    Json(serde_json::Error),
    MyHost,
    ExtractFrame,
    Base,
    #[err(from)]
    TokioJoin(tokio::task::JoinError),
    HostlessUrl,
    #[err(from)]
    Connect(irc_connect::Error),
    UnsupportedScheme(String),
    HttpStatus(StatusCode),
}

struct Bot {
    send_raw: mpsc::UnboundedSender<Vec<u8>>,
    send_raw_receiver: AMutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    send_message: mpsc::UnboundedSender<Message>,
    delay: Duration,
    autojoin: Option<String>,
    auth: Option<HeaderValue>,
    copyparty_url: Url,
    castore: RootCertStore,
    myhost: RwLock<Option<IpAddr>>,
    paths: Mutex<PathIdStore>,
}

impl Bot {
    fn new(
        autojoin: Option<String>,
        auth: Option<HeaderValue>,
        copyparty_url: Url,
        delay: u64,
        castore: RootCertStore,
    ) -> Arc<Self> {
        let (send_raw, send_raw_receiver) = mpsc::unbounded_channel();
        let send_raw_receiver = AMutex::new(send_raw_receiver);
        let (send_message, mut send_message_receiver) = mpsc::unbounded_channel();
        let delay = Duration::from_millis(delay);

        let send_raw_ = send_raw.clone();
        tokio::spawn(async move {
            while let Some(Message { target, content }) = send_message_receiver.recv().await {
                let line = Line {
                    tags: None,
                    source: None,
                    command: "PRIVMSG".to_string(),
                    arguments: vec![target, content],
                };
                _ = send_raw_.send(line.format());
                tokio::time::sleep(delay).await;
            }
        });

        Arc::new(Self {
            send_raw,
            send_raw_receiver,
            send_message,
            delay,
            autojoin,
            auth,
            copyparty_url,
            castore,
            myhost: RwLock::new(None),
            paths: Mutex::new(PathIdStore::new()),
        })
    }

    async fn connect(self: &Arc<Self>, nick: &str, addr: &str, tls: bool, autoreconn: Option<u64>) {
        let delay = autoreconn.map(Duration::from_secs);
        loop {
            if let Err(e) = self.connect_once(nick, addr, tls).await {
                eprintln!("connection failed: {e}");
            } else {
                eprintln!("disconnected gracefully...");
            }
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
                eprintln!("reconnecting...");
            } else {
                break;
            }
        }
    }

    async fn connect_once(
        self: &Arc<Self>,
        nick: &str,
        addr: &str,
        tls: bool,
    ) -> Result<(), Error> {
        let conn = Connection::new_tcp(addr);
        let conn = if tls {
            conn.tls_with_root(None, self.castore.clone())
        } else {
            conn
        };
        let conn = conn.connect().await?;
        let (reader, mut writer) = tokio::io::split(conn);
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::with_capacity(512);
        let mut send_raw_receiver = self.send_raw_receiver.lock().await;

        writer
            .write_all(format!("NICK {nick}\r\nUSER ciao 0 * :offerparty\r\n").as_bytes())
            .await?;

        loop {
            tokio::select! {
                Ok(len) = reader.read_until(b'\n', &mut buf) => {
                    if len == 0 {
                        return Ok(());
                    }
                    while buf.pop_if(|c| b"\r\n".contains(c)).is_some() {}
                    let mut line = Line::tokenise(&buf)?;
                    buf.clear();
                    line.command.make_ascii_uppercase();

                    match line.command.as_ref() {
                        "001" => self.handle_001(line)?,
                        "302" => self.handle_302(&line),
                        "433" => self.handle_433(line)?,
                        "PRIVMSG" => self.handle_privmsg(&line)?,
                        _ => (),
                    }
                }
                Some(mut bytes) = send_raw_receiver.recv() => {
                    bytes.extend_from_slice(b"\r\n");
                    writer.write_all(&bytes).await?;
                }
                () = tokio::time::sleep(Duration::from_secs(30)) => {
                    self.send_raw.send(b"PING meow".to_vec()).map_err(|_| Error::Send)?;
                }
            };
        }
    }

    fn send_raw(&self, command: String, arguments: Vec<Vec<u8>>) -> Result<(), Error> {
        let res = Line {
            tags: None,
            source: None,
            command,
            arguments,
        };
        self.send_raw.send(res.format()).map_err(|_| Error::Send)
    }

    fn send_message(&self, target: Vec<u8>, content: Vec<u8>) -> Result<(), Error> {
        let Some(msg) = Message::new(target, content) else {
            return Ok(());
        };
        self.send_message.send(msg).map_err(|_| Error::Send)
    }

    fn handle_001(&self, line: Line) -> Result<(), Error> {
        if let Some(mynick) = line.arguments.into_iter().next() {
            self.send_raw("USERHOST".to_string(), vec![mynick])?;
        }
        if let Some(channel) = &self.autojoin {
            self.send_raw("JOIN".to_string(), vec![channel.as_bytes().to_vec()])?;
        }
        Ok(())
    }

    fn handle_302(&self, line: &Line) {
        *self.myhost.write().unwrap() = line
            .arguments
            .get(1)
            .and_then(|a| str::from_utf8(a).ok())
            .and_then(|s| s.split(' ').next())
            .and_then(|s| s.rsplit_once('@'))
            .and_then(|(_, h)| IpAddr::from_str(h).ok());
    }

    fn handle_433(&self, line: Line) -> Result<(), Error> {
        if let Some(mut badnick) = line.arguments.into_iter().nth(1) {
            badnick.push(b'_');
            self.send_raw("NICK".to_string(), vec![badnick])?;
        }

        Ok(())
    }

    fn handle_privmsg(self: &Arc<Self>, line: &Line) -> Result<(), Error> {
        let Some(source) = line
            .source
            .as_ref()
            .and_then(|s| s.split(|&c| c == b'!').next())
        else {
            return Ok(());
        };
        if let Some(target) = line
            .arguments
            .first()
            .filter(|t| t.starts_with(b"#"))
            .map(|v| &v[..])
        {
            if line.arguments.get(1).is_some_and(|t| t == b"!list") {
                self.send_message(
                    target.to_vec(),
                    format!(
                        "ciao a tutti! message me XDCC HELP to download stuff from {}",
                        self.copyparty_url
                    )
                    .into_bytes(),
                )?;
            }
            return Ok(());
        }
        if let Some(text) = line.arguments.get(1).and_then(|s| str::from_utf8(s).ok())
            && let Some((first, rest)) = text.split_once(' ')
            && first.eq_ignore_ascii_case("XDCC")
        {
            let (first, rest) = rest.split_once(' ').unwrap_or((rest, ""));
            match first.to_ascii_uppercase().as_str() {
                "HELP" => {
                    self.send_message(source.to_vec(), b"commands: XDCC HELP - this text, XDCC LIST [number] - list stuff to download, XDCC SEND <number> - get a file".to_vec())?;
                }
                "LIST" => {
                    let target = source.to_vec();
                    let rest = rest.to_string();
                    let myself = Arc::clone(self);
                    tokio::spawn(async move {
                        myself
                            .message_errors(&target, myself.do_list(&target, &rest))
                            .await;
                    });
                }
                "SEND" => {
                    let target = source.to_vec();
                    let rest = rest.to_string();
                    let myself = Arc::clone(self);
                    tokio::spawn(async move {
                        myself
                            .message_errors(&target, myself.do_send(&target, &rest))
                            .await;
                    });
                }
                _ => (),
            }
        }
        Ok(())
    }

    async fn message_errors<F>(&self, target: &[u8], future: F)
    where
        F: Future<Output = Result<(), Error>> + Send,
    {
        let Ok(res) = tokio::time::timeout(Duration::from_mins(10), future).await else {
            _ = self.send_message(target.to_vec(), b"timed out".to_vec());
            return;
        };
        if let Err(e) = res {
            let formatted = format!("oh noes: {e}");
            for line in formatted.lines() {
                let mut line = line.as_bytes().to_vec();
                if line.is_empty() {
                    line.push(b' ');
                }
                if self.send_message(target.to_vec(), line).is_err() {
                    break;
                }
            }
        }
    }

    fn get_path(&self, num: &str) -> Result<String, Error> {
        let num = if num.is_empty() {
            0
        } else {
            usize::from_str(num)?
        };

        self.paths
            .lock()
            .unwrap()
            .get_path(num)
            .map(str::to_string)
            .ok_or(Error::UnknownId)
    }

    async fn do_list(&self, target: &[u8], num: &str) -> Result<(), Error> {
        let path = self.get_path(num)?;
        if !path.is_empty() && !path.ends_with('/') {
            return Err(Error::NotADirectory(path));
        }
        let mut url = self.copyparty_url.join(&path)?;
        // ask copyparty for json
        url.set_query(Some("ls"));
        let resp = self.http_get(url, self.auth.as_ref()).await?;
        let body = resp.collect().await?.aggregate();
        let dir: Directory = serde_json::from_reader(body.reader())?;

        self.send_message(
            target.to_vec(),
            format!(
                "{path:?} has {} directories and {} files:",
                dir.dirs.len(),
                dir.files.len()
            )
            .into_bytes(),
        )?;

        for entry in dir.dirs.iter().chain(dir.files.iter()) {
            let fullpath = format!("{path}{}", entry.name);
            let id = self.paths.lock().unwrap().generate_id(&fullpath);
            self.send_message(
                target.to_vec(),
                format!("#{id} [{}] {fullpath}", Human(entry.size)).into_bytes(),
            )?;
            tokio::time::sleep(self.delay).await;
        }

        Ok(())
    }

    async fn do_send(&self, target: &[u8], num: &str) -> Result<(), Error> {
        let path = self.get_path(num)?;
        let mut url = self.copyparty_url.join(&path)?;
        if path.is_empty() {
            return Err(Error::Base);
        }
        if path.ends_with('/') {
            url.set_query(Some("zip=crc"));
        }
        let resp = self.http_get(url, self.auth.as_ref()).await?;
        if resp.status() != StatusCode::OK {
            return Err(Error::HttpStatus(resp.status()));
        }
        let len = resp
            .headers()
            .get("Content-Length")
            .and_then(|h| h.to_str().ok());
        let Some(myhost) = self.myhost.read().unwrap().as_ref().copied() else {
            return Err(Error::MyHost);
        };

        let mut segments = path.rsplit('/');
        let name = segments.next().unwrap_or("unknown");
        let name = if name.is_empty() {
            let dirname = match segments.next() {
                None | Some("") => "directory",
                Some(d) => d,
            };
            format!("{dirname}.zip")
        } else {
            name.to_string()
        };

        let (mut sock, _) = {
            let listener = tokio::net::TcpListener::bind((myhost, 0)).await?;
            let addr = listener.local_addr()?;

            self.send_message(
                target.to_vec(),
                format!(
                    "\x01DCC SEND {name} {} {}{}\x01",
                    DccIp(addr.ip()),
                    addr.port(),
                    MaybeSpace(len),
                )
                .into_bytes(),
            )?;

            listener.accept().await?
        };

        let mut body = resp.into_body();
        let mut hasher = Sha256::new();

        while let Some(frame) = body.frame().await {
            let data = &frame?.into_data().map_err(|_| Error::ExtractFrame)?;
            sock.write_all(data).await?;
            hasher.update(data);
        }

        sock.shutdown().await?;

        let hash = hasher.finalize();
        self.send_message(
            target.to_vec(),
            format!("successfully sent {name} (sha256 {})", Hex(&hash)).into_bytes(),
        )?;

        Ok(())
    }

    async fn http_get(
        &self,
        mut url: Url,
        auth: Option<&HeaderValue>,
    ) -> Result<Response<Incoming>, Error> {
        if !url.has_host() {
            return Err(Error::HostlessUrl);
        }
        if url.path().is_empty() {
            // Url normalizes this automatically for http(s) but not
            // other schemes >:(
            url.set_path("/");
        }

        let (addrs, url) = tokio::task::spawn_blocking(move || {
            (
                url.socket_addrs(|| (url.scheme() == "https+insecure").then_some(443)),
                url,
            )
        })
        .await?;
        let addrs = addrs?;

        let stream = Connection::new_tcp(addrs.first().unwrap());

        let stream = match url.scheme() {
            "https" => stream.tls_with_root(
                url.host_str()
                    .and_then(|h| ServerName::try_from(h).ok())
                    .map(|h| h.to_owned()),
                self.castore.clone(),
            ),
            "https+insecure" => stream.tls_danger_insecure(None),
            "http" => stream,
            unknown => return Err(Error::UnsupportedScheme(unknown.to_string())),
        };

        let stream = stream.connect().await?;
        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

        tokio::spawn(async move {
            _ = conn.await;
        });

        let req = Request::builder()
            .uri(&url[url::Position::BeforePath..url::Position::AfterQuery])
            .header(hyper::header::HOST, url.host_str().unwrap())
            .header(
                hyper::header::USER_AGENT,
                concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
            );

        let req = if let Some(auth) = auth {
            req.header(hyper::header::AUTHORIZATION, auth)
        } else {
            req
        };

        let req = req.body(Empty::<Bytes>::new())?;

        Ok(sender.send_request(req).await?)
    }
}

struct Message {
    target: Vec<u8>,
    content: Vec<u8>,
}

impl Message {
    fn new(target: Vec<u8>, content: Vec<u8>) -> Option<Self> {
        if target.iter().any(|c| b"\r\n\0 ".contains(c))
            || content.iter().any(|c| b"\r\n\0".contains(c))
        {
            return None;
        }
        Some(Self { target, content })
    }
}

struct PathIdStore {
    by_path: HashMap<String, usize>,
    by_id: Vec<String>,
}

impl PathIdStore {
    fn new() -> Self {
        let mut by_path = HashMap::new();
        by_path.insert(String::new(), 0);
        let by_id = vec![String::new()];
        Self { by_path, by_id }
    }
    fn generate_id(&mut self, path: &str) -> usize {
        if let Some(&id) = self.by_path.get(path) {
            return id;
        }
        let new_id = self.by_id.len();
        self.by_path.insert(path.to_string(), new_id);
        self.by_id.push(path.to_string());
        new_id
    }
    fn get_path(&self, id: usize) -> Option<&str> {
        self.by_id.get(id).map(String::as_str)
    }
}

#[derive(Debug, Deserialize)]
struct Directory {
    dirs: Vec<DirEntry>,
    files: Vec<DirEntry>,
}

#[derive(Debug, Deserialize)]
struct DirEntry {
    #[serde(rename = "href")]
    name: String,
    #[serde(rename = "sz")]
    size: u64,
}

struct Human(u64);

impl fmt::Display for Human {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            return write!(f, "0");
        }
        match self.0.ilog2() {
            60.. => write!(f, "{}EiB", self.0 >> 60),
            50.. => write!(f, "{}PiB", self.0 >> 50),
            40.. => write!(f, "{}TiB", self.0 >> 40),
            30.. => write!(f, "{}GiB", self.0 >> 30),
            20.. => write!(f, "{}MiB", self.0 >> 20),
            10.. => write!(f, "{}KiB", self.0 >> 10),
            _ => write!(f, "{}B", self.0),
        }
    }
}

struct DccIp(IpAddr);

impl fmt::Display for DccIp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            IpAddr::V4(ipv4) => write!(f, "{}", ipv4.to_bits()),
            IpAddr::V6(ipv6) => write!(f, "{ipv6}"),
        }
    }
}

struct MaybeSpace<'a>(Option<&'a str>);

impl fmt::Display for MaybeSpace<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(inner) = self.0 {
            write!(f, " {inner}")?;
        }
        Ok(())
    }
}

struct Hex<'a>(&'a [u8]);

impl fmt::Display for Hex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let opt: Opt = argh::from_env();

    let mut castore = RootCertStore::empty();
    castore.add_parsable_certificates(CertificateDer::pem_file_iter(opt.trust).unwrap().flatten());

    let bot = Bot::new(opt.join, opt.auth, opt.url, opt.delay, castore);

    bot.connect(&opt.nick, &opt.addr, opt.tls, opt.autoconn)
        .await;
}
