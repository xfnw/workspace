// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use argh::{FromArgs, from_env};
use hashlink::LruCache;
use irc_connect::Stream;
use irctokens::Line;
use rand::{seq::SliceRandom, thread_rng};
use std::{path::PathBuf, sync::Mutex};
use tokio::{
    io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    sync::Mutex as AMutex,
};

/// content addressed irc bot
#[derive(Debug, FromArgs)]
struct Opt {
    /// number of lines to cache, defaults to 1000
    #[argh(option, short = 'c', default = "1000")]
    capacity: usize,
    /// nickname to use
    #[argh(option, short = 'n', default = "\"ca\".to_string()")]
    nick: String,
    /// channel to automatically join
    #[argh(option, short = 'j', default = "\"#ca\".to_string()")]
    join: String,
    /// enable tls
    #[argh(switch, short = 't')]
    tls: bool,
    /// path of certificate store to trust
    #[argh(
        option,
        short = 'T',
        default = "\"/etc/ssl/certs/ca-bundle.crt\".into()"
    )]
    trust: PathBuf,
    /// irc server address to connect to
    #[argh(positional)]
    addr: String,
}

struct Bot {
    read: AMutex<BufReader<ReadHalf<Stream>>>,
    write: AMutex<WriteHalf<Stream>>,
    nick: Mutex<Option<Vec<u8>>>,
    join: String,
    cache: Mutex<LruCache<[u8; 16], Vec<u8>>>,
}

impl Bot {
    fn new(stream: Stream, join: String, capacity: usize) -> Self {
        let (read, write) = io::split(stream);

        Self {
            read: AMutex::new(BufReader::new(read)),
            write: AMutex::new(write),
            nick: Mutex::new(None),
            join,
            cache: Mutex::new(LruCache::new(capacity)),
        }
    }

    async fn write_line(&self, line: &Line) -> io::Result<()> {
        let mut output = line.format();
        output.extend_from_slice(b"\r\n");
        let mut writer = self.write.lock().await;
        writer.write_all(&output).await
    }

    async fn run(&self) -> io::Result<()> {
        let mut buf = Vec::with_capacity(512);

        loop {
            buf.clear();
            let len = self.read.lock().await.read_until(b'\n', &mut buf).await?;
            if len == 0 {
                return Ok(());
            }
            while buf.pop_if(|c| b"\r\n".contains(c)).is_some() {}
            let Ok(mut line) = Line::tokenise(&buf) else {
                continue;
            };
            line.command.make_ascii_uppercase();

            match line.command.as_ref() {
                "001" => self.handle_001(line).await?,
                "433" => self.handle_433(line).await?,
                "NICK" => self.handle_nick(&line),
                "PING" => self.handle_ping(line).await?,
                "PRIVMSG" => self.handle_privmsg(line).await?,
                _ => (),
            }
        }
    }

    async fn handle_001(&self, line: Line) -> io::Result<()> {
        *self.nick.lock().unwrap() = line.arguments.first().cloned();
        let join = Line {
            tags: None,
            source: None,
            command: "JOIN".to_string(),
            arguments: vec![self.join.as_bytes().to_vec()],
        };
        self.write_line(&join).await?;
        Ok(())
    }

    async fn handle_433(&self, line: Line) -> io::Result<()> {
        const NICK_CHARS: &[u8] = b"[\\]_|0123456789abcdefghijklmnopqrstuvwxyz";
        if let Some(mut badnick) = line.arguments.into_iter().nth(1) {
            let mut rng = thread_rng();
            badnick.push(*NICK_CHARS.choose(&mut rng).unwrap());

            let res = Line {
                tags: None,
                source: None,
                command: "NICK".to_string(),
                arguments: vec![badnick],
            };
            self.write_line(&res).await?;
        }

        Ok(())
    }

    fn handle_nick(&self, line: &Line) {
        if let Some(source_nick) = line
            .source
            .as_ref()
            .and_then(|s| s.split(|&b| b == b'!').next())
            && let mut mynick = self.nick.lock().unwrap()
            && mynick.as_ref().is_some_and(|n| n == source_nick)
        {
            *mynick = line.arguments.first().cloned();
        }
    }

    async fn handle_ping(&self, line: Line) -> io::Result<()> {
        let pong = Line {
            tags: None,
            source: None,
            command: "PONG".to_string(),
            arguments: line.arguments,
        };
        self.write_line(&pong).await?;
        Ok(())
    }

    async fn handle_privmsg(&self, mut line: Line) -> io::Result<()> {
        let (Some(message), Some(target)) = (line.arguments.pop(), line.arguments.pop()) else {
            return Ok(());
        };
        let target = if target.first().is_some_and(|&c| c == b'#') {
            target
        } else {
            let Some(source_nick) = line
                .source
                .as_ref()
                .and_then(|s| s.split(|&b| b == b'!').next())
            else {
                return Ok(());
            };
            source_nick.to_vec()
        };

        if let Some(digest) = unhex_digest(&message)
            && let Some(contents) = { self.cache.lock().unwrap().get(&digest).cloned() }
        {
            let res = Line {
                tags: None,
                source: None,
                command: "PRIVMSG".to_string(),
                arguments: vec![target, contents],
            };
            self.write_line(&res).await?;
        }

        let digest = md5::compute(&message).0;
        self.cache.lock().unwrap().insert(digest, message);
        Ok(())
    }
}

fn unhex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 0xa),
        _ => None,
    }
}

fn unhex_digest(inp: &[u8]) -> Option<[u8; 16]> {
    if inp.len() != 32 {
        return None;
    }

    let (chunks, []) = inp.as_chunks::<2>() else {
        panic!("32 should be a multiple of 2");
    };

    let mut out = [0; 16];

    for (i, &[h, l]) in chunks.iter().enumerate() {
        out[i] = (unhex_nibble(h)? << 4) | unhex_nibble(l)?;
    }

    Some(out)
}

#[test]
fn check_unhex_digest() {
    let expect = [
        0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd,
        0xef,
    ];
    assert_eq!(
        unhex_digest(b"1234567890abcdef1234567890abcdef"),
        Some(expect)
    );
}

#[tokio::main]
async fn main() {
    let opt: Opt = from_env();

    let stream = Stream::new_tcp(opt.addr);
    let mut stream = stream.connect().await.unwrap();

    stream
        .write_all(
            format!(
                "NICK {}\r\nUSER ca 0 * :content addressed bot\r\n",
                opt.nick
            )
            .as_bytes(),
        )
        .await
        .unwrap();

    let bot = Bot::new(stream, opt.join, opt.capacity);

    bot.run().await.unwrap();
}
