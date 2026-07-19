// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use crate::{Error, tohex_digest, unhex_digest};
use hashlink::LruCache;
use irc_connect::Connection;
use irctokens::Line;
use rand::{Rng, seq::SliceRandom, thread_rng};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::{
    io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    sync::{Mutex as AMutex, broadcast},
    time::sleep,
};

pub struct Bot {
    read: AMutex<BufReader<ReadHalf<Connection>>>,
    write: AMutex<WriteHalf<Connection>>,
    channel: String,
    delay: u64,
    stealthy: bool,
    last_sent: Mutex<Instant>,
    cache: Mutex<LruCache<[u8; 16], Vec<u8>>>,
    pin_cache: Mutex<LruCache<[u8; 16], Vec<u8>>>,
    digest_firehose: broadcast::Sender<[u8; 16]>,
}

impl Bot {
    pub fn new(
        stream: Connection,
        join: String,
        delay: u64,
        capacity: usize,
        stealthy: bool,
    ) -> Self {
        let (read, write) = io::split(stream);

        Self {
            read: AMutex::new(BufReader::new(read)),
            write: AMutex::new(write),
            channel: join,
            delay,
            stealthy,
            last_sent: Mutex::new(Instant::now()),
            cache: Mutex::new(LruCache::new(capacity)),
            pin_cache: Mutex::new(LruCache::new_unbounded()),
            digest_firehose: broadcast::Sender::new(256),
        }
    }

    pub async fn write_line(&self, line: &Line) -> Result<(), Error> {
        *self.last_sent.lock().unwrap() = Instant::now();

        let mut output = line.format();
        output.extend_from_slice(b"\r\n");
        let mut writer = self.write.lock().await;
        writer.write_all(&output).await?;

        Ok(())
    }

    pub async fn run(&self) -> Result<(), Error> {
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
                "001" => self.handle_001().await?,
                "433" => self.handle_433(line).await?,
                "PING" => self.handle_ping(line).await?,
                "PRIVMSG" => self.handle_privmsg(line).await?,
                _ => (),
            }
        }
    }

    async fn handle_001(&self) -> Result<(), Error> {
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
                .or_else(|| self.pin_cache.lock().unwrap().get(&digest).cloned())
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

    pub async fn send_many(&self, messages: Vec<Vec<u8>>) -> Result<(), Error> {
        let mut line = Line {
            tags: None,
            source: None,
            command: "PRIVMSG".to_string(),
            arguments: vec![self.channel.as_bytes().to_vec()],
        };

        for message in messages {
            line.arguments.push(message);

            if !self.stealthy {
                self.write_line(&line).await?;
            }

            let digest = md5::compute(&line.arguments[1]).0;
            self.pin_cache
                .lock()
                .unwrap()
                .insert(digest, line.arguments.pop().unwrap());
            _ = self.digest_firehose.send(digest);

            if !self.stealthy {
                sleep(Duration::from_millis(self.delay)).await;
            }
        }

        Ok(())
    }

    pub async fn retrieve(&self, digest: [u8; 16]) -> Result<Vec<u8>, Error> {
        let mut receiver = self.digest_firehose.subscribe();

        if let Some(content) = { self.cache.lock().unwrap().get(&digest).cloned() }
            .or_else(|| self.pin_cache.lock().unwrap().get(&digest).cloned())
        {
            self.pin_cache
                .lock()
                .unwrap()
                .insert(digest, content.clone());

            return Ok(content);
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
            if let Some(content) = { self.cache.lock().unwrap().get(&digest).cloned() } {
                self.pin_cache
                    .lock()
                    .unwrap()
                    .insert(digest, content.clone());

                return Ok(content);
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

    pub async fn shutdown(&self) -> Result<(), Error> {
        let quit = Line {
            tags: None,
            source: None,
            command: "QUIT".to_string(),
            arguments: vec![b"meow".to_vec()],
        };
        self.write_line(&quit).await
    }
}
