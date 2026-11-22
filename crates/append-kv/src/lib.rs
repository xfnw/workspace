// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

//! append-only key-value database thing that will eat your data
//!
//! this is not very robust and has a rather foot-gun-full api,
//! but it is simple and avoids write amplification

#![expect(clippy::missing_panics_doc)]
#![expect(clippy::missing_errors_doc)]
#![expect(clippy::cast_possible_truncation)]

use patricia_tree::PatriciaMap;
use std::{
    io::{self, SeekFrom},
    sync::RwLock,
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt, BufWriter},
    sync::{Mutex, MutexGuard},
};

pub trait AsyncSetLen {
    fn set_len(&mut self, size: u64) -> impl Future<Output = io::Result<()>>;
}

impl AsyncSetLen for File {
    fn set_len(&mut self, size: u64) -> impl Future<Output = io::Result<()>> {
        Self::set_len(self, size)
    }
}

impl<T> AsyncSetLen for BufWriter<T>
where
    T: AsyncWrite + AsyncSetLen + Unpin,
{
    async fn set_len(&mut self, size: u64) -> io::Result<()> {
        self.flush().await?;
        self.get_mut().set_len(size).await
    }
}

pub struct AppendKV {
    offsets: RwLock<PatriciaMap<u64>>,
    next_offset: Mutex<u64>,
}

impl AppendKV {
    pub async fn open<F>(file: &mut F) -> io::Result<Self>
    where
        F: AsyncRead + AsyncSeek + Unpin,
    {
        let mut offsets = PatriciaMap::new();
        let mut next_offset = 0;

        loop {
            assert_eq!(file.seek(SeekFrom::Start(next_offset)).await?, next_offset);

            let key_len = match file.read_u64().await {
                Ok(l) => l,
                Err(e) => {
                    if e.kind() == io::ErrorKind::UnexpectedEof {
                        break;
                    }
                    return Err(e);
                }
            };
            next_offset += 8 + key_len;
            let mut key = vec![0; key_len as usize];
            file.read_exact(&mut key).await?;

            offsets.insert(key, next_offset);

            let value_len = file.read_u64().await?;
            next_offset += 8 + value_len;
        }

        Ok(Self {
            offsets: RwLock::new(offsets),
            next_offset: Mutex::new(next_offset),
        })
    }

    pub fn contains_key(&self, key: impl AsRef<[u8]>) -> bool {
        self.offsets.read().unwrap().contains_key(key.as_ref())
    }

    pub fn inner(&self) -> std::sync::RwLockReadGuard<'_, PatriciaMap<u64>> {
        self.offsets.read().unwrap()
    }

    pub async fn get<F, K>(&self, file: &mut F, key: K) -> io::Result<u64>
    where
        F: AsyncRead + AsyncSeek + Unpin,
        K: AsRef<[u8]>,
    {
        let Some(offset) = self.offsets.read().unwrap().get(key).copied() else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "key not found"));
        };

        assert_eq!(file.seek(SeekFrom::Start(offset)).await?, offset);
        let length = file.read_u64().await?;

        Ok(length)
    }

    pub async fn get_vec<F, K>(&self, file: &mut F, key: K) -> io::Result<Vec<u8>>
    where
        F: AsyncRead + AsyncSeek + Unpin,
        K: AsRef<[u8]>,
    {
        let size = self.get(file, key).await?;
        let mut out = vec![0; size as usize];
        file.read_exact(&mut out).await?;
        Ok(out)
    }

    async fn insert_with<F>(
        &self,
        file: &mut F,
        key: Vec<u8>,
        size: u64,
        mut next_offset: MutexGuard<'_, u64>,
    ) -> Result<(), io::Error>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
    {
        file.set_len(*next_offset).await?;
        assert_eq!(
            file.seek(SeekFrom::Start(*next_offset)).await?,
            *next_offset
        );

        file.write_u64(key.len() as u64).await?;
        file.write_all(&key).await?;
        file.write_u64(size).await?;

        *next_offset += 8 + key.len() as u64;
        let value_offset = *next_offset;
        *next_offset += 8 + size;

        self.offsets.write().unwrap().insert(key, value_offset);
        Ok(())
    }

    pub async fn insert<F>(&self, file: &mut F, key: Vec<u8>, size: u64) -> io::Result<()>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
    {
        let next_offset = self.next_offset.lock().await;

        self.insert_with(file, key, size, next_offset).await?;

        Ok(())
    }

    pub async fn insert_slice<F>(&self, file: &mut F, key: Vec<u8>, value: &[u8]) -> io::Result<()>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
    {
        self.insert(file, key, value.len() as u64).await?;
        file.write_all(value).await?;
        Ok(())
    }

    pub async fn try_insert<F>(&self, file: &mut F, key: Vec<u8>, size: u64) -> io::Result<()>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
    {
        let next_offset = self.next_offset.lock().await;

        if self.contains_key(&key) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "key already exists",
            ));
        }

        self.insert_with(file, key, size, next_offset).await?;

        Ok(())
    }

    pub async fn try_insert_slice<F>(
        &self,
        file: &mut F,
        key: Vec<u8>,
        value: &[u8],
    ) -> io::Result<()>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
    {
        self.try_insert(file, key, value.len() as u64).await?;
        file.write_all(value).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use crate::AppendKV;
    use tempfile::tempfile;
    use tokio::{
        fs::File,
        io::{AsyncWriteExt, BufWriter},
    };

    #[tokio::test]
    async fn round_trip() {
        let file = tempfile().unwrap();
        let mut file = File::from_std(file);

        let kv = AppendKV::open(&mut file).await.unwrap();
        kv.try_insert_slice(&mut file, b"meow".to_vec(), b"woof")
            .await
            .unwrap();
        assert_eq!(
            kv.try_insert_slice(&mut file, b"meow".to_vec(), b"yip")
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );

        let mut buffered = BufWriter::new(file);
        kv.insert_slice(&mut buffered, b"wisteria noise".to_vec(), b"woem")
            .await
            .unwrap();
        kv.insert_slice(&mut buffered, b"wisteria noise".to_vec(), b"meow")
            .await
            .unwrap();
        buffered.flush().await.unwrap();

        let mut file = buffered.into_inner();
        let kv = AppendKV::open(&mut file).await.unwrap();
        assert_eq!(
            kv.get_vec(&mut file, b"wisteria noise").await.unwrap(),
            b"meow"
        );
        assert_eq!(kv.get_vec(&mut file, b"meow").await.unwrap(), b"woof");
        assert_eq!(
            kv.get_vec(&mut file, b"mow").await.unwrap_err().kind(),
            io::ErrorKind::NotFound
        );

        assert_eq!(
            kv.inner().keys().collect::<Vec<_>>(),
            [b"meow".as_ref(), b"wisteria noise".as_ref()]
        );
    }
}
