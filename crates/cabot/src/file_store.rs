// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use crate::{Error, bot::Bot, tohex_digest, unhex_digest};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD as BASE64};
use std::sync::Arc;

pub struct FileStore {
    bot: Arc<Bot>,
}

impl FileStore {
    pub fn new(bot: Arc<Bot>) -> Self {
        Self { bot }
    }

    pub async fn store(&self, file: &[u8]) -> Result<[u8; 16], Error> {
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

    pub async fn retrieve(&self, digest: [u8; 16]) -> Result<Vec<u8>, Error> {
        let mut futures = vec![];
        let mut next_hash = digest;

        loop {
            let hashes = self.bot.retrieve(next_hash).await?;
            let Some(hashes): Option<Vec<_>> = hashes.chunks(32).map(unhex_digest).collect() else {
                return Err(Error::FileInvalidHashes);
            };

            for &hash in &hashes[0..std::cmp::min(hashes.len(), 10)] {
                let bot = self.bot.clone();
                futures.push(tokio::spawn(async move { bot.retrieve(hash).await }));
            }

            match hashes.len() {
                11 => {
                    next_hash = hashes[10];
                }
                ..=10 => {
                    break;
                }
                _ => {
                    return Err(Error::FileTooManyHashes);
                }
            }
        }

        let mut result = vec![];

        for future in futures {
            let data = future.await.unwrap()?;
            if data != b"empty" {
                result.extend_from_slice(&BASE64.decode(data)?);
            }
        }

        Ok(result)
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        self.bot.quit().await
    }
}
