// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use argh::{FromArgs, from_env};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD as BASE64};
use hashlink::LruCache;
use irc_connect::{
    Stream,
    tokio_rustls::rustls::{
        RootCertStore,
        pki_types::{CertificateDer, pem::PemObject},
    },
};
use irctokens::Line;
use rand::{Rng, seq::SliceRandom, thread_rng};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    sync::{Mutex as AMutex, broadcast},
    time::sleep,
};

/// content addressed irc bot
#[derive(Debug, FromArgs)]
struct Opt {
    /// number of lines to cache, defaults to 1000
    #[argh(option, short = 'c', default = "1000")]
    capacity: usize,
    /// number of milliseconds to mostly wait between sending messages
    #[argh(option, short = 'd', default = "0")]
    delay: u64,
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

#[derive(Debug, foxerror::FoxError)]
enum Error {
    #[err(from)]
    Io(io::Error),
    Broadcast(broadcast::error::RecvError),
}

struct Bot {
    read: AMutex<BufReader<ReadHalf<Stream>>>,
    write: AMutex<WriteHalf<Stream>>,
    nick: Mutex<Option<Vec<u8>>>,
    channel: String,
    delay: u64,
    last_sent: Mutex<Instant>,
    cache: Mutex<LruCache<[u8; 16], Vec<u8>>>,
    digest_firehose: broadcast::Sender<[u8; 16]>,
}

impl Bot {
    fn new(stream: Stream, join: String, delay: u64, capacity: usize) -> Arc<Self> {
        let (read, write) = io::split(stream);

        Arc::new(Self {
            read: AMutex::new(BufReader::new(read)),
            write: AMutex::new(write),
            nick: Mutex::new(None),
            channel: join,
            delay,
            last_sent: Mutex::new(Instant::now()),
            cache: Mutex::new(LruCache::new(capacity)),
            digest_firehose: broadcast::Sender::new(256),
        })
    }

    async fn write_line(&self, line: &Line) -> Result<(), Error> {
        *self.last_sent.lock().unwrap() = Instant::now();

        let mut output = line.format();
        output.extend_from_slice(b"\r\n");
        let mut writer = self.write.lock().await;
        writer.write_all(&output).await?;

        Ok(())
    }

    async fn run(&self) -> Result<(), Error> {
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

    async fn handle_001(&self, line: Line) -> Result<(), Error> {
        *self.nick.lock().unwrap() = line.arguments.first().cloned();
        let join = Line {
            tags: None,
            source: None,
            command: "JOIN".to_string(),
            arguments: vec![self.channel.as_bytes().to_vec()],
        };
        self.write_line(&join).await?;
        Ok(())
    }

    async fn handle_433(&self, line: Line) -> Result<(), Error> {
        const NICK_CHARS: &[u8] = b"[\\]_|0123456789abcdefghijklmnopqrstuvwxyz";
        if let Some(mut badnick) = line.arguments.into_iter().nth(1) {
            badnick.push(*NICK_CHARS.choose(&mut thread_rng()).unwrap());

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

    async fn handle_ping(&self, line: Line) -> Result<(), Error> {
        let pong = Line {
            tags: None,
            source: None,
            command: "PONG".to_string(),
            arguments: line.arguments,
        };
        self.write_line(&pong).await?;
        Ok(())
    }

    async fn handle_privmsg(&self, mut line: Line) -> Result<(), Error> {
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

        #[expect(clippy::cast_possible_truncation)]
        if let Some(digest) = unhex_digest(&message)
            && {
                Instant::now()
                    .duration_since(*self.last_sent.lock().unwrap())
                    .as_millis() as u64
            } >= self.delay
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
        _ = self.digest_firehose.send(digest);
        Ok(())
    }

    async fn send_many(&self, messages: Vec<Vec<u8>>) -> Result<(), Error> {
        let mut line = Line {
            tags: None,
            source: None,
            command: "PRIVMSG".to_string(),
            arguments: vec![self.channel.as_bytes().to_vec(), vec![]],
        };

        for message in messages {
            line.arguments[1] = message;
            self.write_line(&line).await?;
            sleep(Duration::from_millis(self.delay)).await;
        }

        Ok(())
    }

    async fn retrieve(&self, digest: [u8; 16]) -> Result<Vec<u8>, Error> {
        let mut receiver = self.digest_firehose.subscribe();

        if let Some(content) = self.cache.lock().unwrap().get(&digest) {
            return Ok(content.clone());
        }

        sleep(Duration::from_millis({
            thread_rng().gen_range(0..=self.delay * self.digest_firehose.receiver_count() as u64)
        }))
        .await;

        let mut timeout = 1;
        let req = Line {
            tags: None,
            source: None,
            command: "PRIVMSG".to_string(),
            arguments: vec![
                self.channel.as_bytes().to_vec(),
                tohex_digest(digest).to_vec(),
            ],
        };

        loop {
            if let Some(content) = self.cache.lock().unwrap().get(&digest) {
                return Ok(content.clone());
            }

            self.write_line(&req).await?;

            if let Ok(r) = tokio::time::timeout(Duration::from_secs(timeout), async {
                while receiver.recv().await? != digest {}
                Ok(())
            })
            .await
            {
                r.map_err(Error::Broadcast)?;
            }

            timeout *= 2;
            timeout += thread_rng().gen_range(0..10);
        }
    }
}

fn tohex_nibble(n: u8) -> u8 {
    match n {
        0..=9 => n + b'0',
        0xa..=0xf => n + b'a' - 0xa,
        _ => panic!("that is not a nibble"),
    }
}

fn tohex_digest(inp: [u8; 16]) -> [u8; 32] {
    let mut out = [0; 32];

    for (i, b) in inp.iter().enumerate() {
        out[i * 2] = tohex_nibble(b >> 4);
        out[i * 2 + 1] = tohex_nibble(b & 0b1111);
    }

    out
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

#[test]
fn digest_round_trip() {
    assert_eq!(
        tohex_digest(unhex_digest(b"33c6c2397a1b079e903c474df792d0e2").unwrap()),
        *b"33c6c2397a1b079e903c474df792d0e2"
    );
}

struct FileStore {
    bot: Arc<Bot>,
}

impl FileStore {
    fn new(bot: Arc<Bot>) -> Self {
        Self { bot }
    }

    async fn store(&self, file: Vec<u8>) -> Result<[u8; 16], Error> {
        let mut contents = BASE64.encode(file).into_bytes();
        if contents.is_empty() {
            // we cannot send an empty irc message.
            // "empty" is not valid base64 so it should be unambiguous
            contents.extend_from_slice(b"empty");
        }
        let chunks: Vec<_> = contents.chunks(360).collect();
        let hashes: Vec<_> = chunks.iter().map(md5::compute).map(|d| d.0).collect();
        let mut lines = vec![];

        for hash_chunk in hashes.chunks(10).rev() {
            let mut line = vec![];

            for hash in hash_chunk {
                line.extend_from_slice(&tohex_digest(*hash));
            }

            if let Some(prev) = lines.last() {
                line.extend_from_slice(&tohex_digest(md5::compute(prev).0));
            }

            lines.push(line);
        }

        let last_hash = md5::compute(lines.last().unwrap()).0;

        for chunk in chunks {
            lines.push(chunk.to_vec());
        }

        self.bot.send_many(lines).await?;

        Ok(last_hash)
    }
}

#[tokio::main]
async fn main() {
    let opt: Opt = from_env();

    let stream = Stream::new_tcp(opt.addr);
    let stream = if opt.tls {
        let mut root = RootCertStore::empty();
        root.add_parsable_certificates(CertificateDer::pem_file_iter(opt.trust).unwrap().flatten());
        stream.tls_with_root(None, root)
    } else {
        stream
    };
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

    let bot = Bot::new(stream, opt.join, opt.delay, opt.capacity);

    bot.run().await.unwrap();
}
