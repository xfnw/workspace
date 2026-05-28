// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: EUPL-1.2

use argh::FromArgs;
use irctokens::Line;
use std::{net::IpAddr, str::FromStr, sync::RwLock, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpStream, ToSocketAddrs},
    sync::{Mutex as AMutex, mpsc},
};
use url::Url;

/// copyparty to xdcc bridge
#[derive(Debug, FromArgs)]
struct Opt {
    /// milliseconds to wait between sending messages
    #[argh(option, default = "0")]
    delay: u64,
    /// channel to autojoin
    #[argh(option)]
    join: Option<String>,
    /// nickname to use
    #[argh(option, default = "\"offerparty\".to_string()")]
    nick: String,
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
}

struct Bot {
    send_raw: mpsc::UnboundedSender<Vec<u8>>,
    send_raw_receiver: AMutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    send_message: mpsc::UnboundedSender<Message>,
    autojoin: Option<String>,
    copyparty_url: Url,
    myhost: RwLock<Option<IpAddr>>,
}

impl Bot {
    fn new(autojoin: Option<String>, copyparty_url: Url, delay: u64) -> Self {
        let (send_raw, send_raw_receiver) = mpsc::unbounded_channel();
        let send_raw_receiver = AMutex::new(send_raw_receiver);
        let (send_message, mut send_message_receiver) = mpsc::unbounded_channel();

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
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        });

        Self {
            send_raw,
            send_raw_receiver,
            send_message,
            autojoin,
            copyparty_url,
            myhost: RwLock::new(None),
        }
    }

    async fn connect_once(&self, nick: &str, addr: impl ToSocketAddrs) -> Result<(), Error> {
        let conn = TcpStream::connect(addr).await?;
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

    fn handle_privmsg(&self, line: &Line) -> Result<(), Error> {
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
                    b"ciao a tutti! message me XDCC HELP".to_vec(),
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
                    self.send_message(source.to_vec(), b"commands: XDCC HELP - this text, XDCC LIST [directory] - list stuff to download, XDCC SEND <number> - get a file".to_vec())?;
                }
                "LIST" => self.send_message(source.to_vec(), b"not implemented yet".to_vec())?,
                "SEND" => {
                    self.send_message(source.to_vec(), b"also not implemented yet".to_vec())?;
                }
                _ => (),
            }
        }
        Ok(())
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

#[tokio::main]
async fn main() {
    let opt: Opt = argh::from_env();
    let bot = Bot::new(opt.join, opt.url, opt.delay);
    bot.connect_once(&opt.nick, &opt.addr).await.unwrap();
}
