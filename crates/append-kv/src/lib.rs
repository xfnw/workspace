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
        let end = file.seek(SeekFrom::End(0)).await?;
        let mut offsets = PatriciaMap::new();
        let mut next_offset = 0;

        while next_offset < end {
            assert_eq!(file.seek(SeekFrom::Start(next_offset)).await?, next_offset);

            let key_len = file.read_u64().await?;
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

    pub async fn get_pos<F, K>(&self, file: &mut F, key: K) -> io::Result<(u64, u64)>
    where
        F: AsyncRead + AsyncSeek + Unpin,
        K: AsRef<[u8]>,
    {
        let Some(offset) = self.offsets.read().unwrap().get(key).copied() else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "key not found"));
        };

        assert_eq!(file.seek(SeekFrom::Start(offset)).await?, offset);
        let length = file.read_u64().await?;

        Ok((offset + 8, length))
    }

    pub async fn get<F, K>(&self, file: &mut F, key: K) -> io::Result<u64>
    where
        F: AsyncRead + AsyncSeek + Unpin,
        K: AsRef<[u8]>,
    {
        Ok(self.get_pos(file, key).await?.1)
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

    async fn insert_at<F>(
        &self,
        file: &mut F,
        key: &[u8],
        size: u64,
        next_offset: &mut MutexGuard<'_, u64>,
    ) -> Result<u64, io::Error>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
    {
        let data_offset = **next_offset + key.len() as u64 + 8;
        let new_len = data_offset + size + 8;
        file.set_len(new_len).await?;
        assert_eq!(
            file.seek(SeekFrom::Start(**next_offset)).await?,
            **next_offset
        );

        file.write_u64(key.len() as u64).await?;
        file.write_all(key).await?;
        file.write_u64(size).await?;

        **next_offset = new_len;
        Ok(data_offset)
    }

    pub async fn insert<F, K>(&self, file: &mut F, key: K, size: u64) -> io::Result<()>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
        K: AsRef<[u8]>,
    {
        let data_offset = {
            let mut next_offset = self.next_offset.lock().await;
            let data_offset = self
                .insert_at(file, key.as_ref(), size, &mut next_offset)
                .await?;
            file.flush().await?;
            data_offset
        };

        self.offsets.write().unwrap().insert(key, data_offset);

        Ok(())
    }

    pub async fn insert_consistent<F, K, C, O>(
        &self,
        file: &mut F,
        key: K,
        size: u64,
        callback: C,
    ) -> io::Result<O>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
        K: AsRef<[u8]>,
        C: AsyncFnOnce(&mut F) -> O,
    {
        let data_offset = self
            .insert_at(file, key.as_ref(), size, &mut self.next_offset.lock().await)
            .await?;

        let out = callback(file).await;

        file.flush().await?;

        self.offsets.write().unwrap().insert(key, data_offset);

        Ok(out)
    }

    pub async fn insert_slice<F, K>(&self, file: &mut F, key: K, value: &[u8]) -> io::Result<()>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
        K: AsRef<[u8]>,
    {
        self.insert_consistent(file, key, value.len() as u64, async |f| {
            f.write_all(value).await
        })
        .await?
    }

    pub async fn try_insert<F, K>(&self, file: &mut F, key: K, size: u64) -> io::Result<()>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
        K: AsRef<[u8]>,
    {
        let mut next_offset = self.next_offset.lock().await;

        // not leaving self.offsets locked here is fine since
        // the next_offset lock already prevents new inserts
        if self.contains_key(&key) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "key already exists",
            ));
        }

        let data_offset = self
            .insert_at(file, key.as_ref(), size, &mut next_offset)
            .await?;

        file.flush().await?;

        self.offsets.write().unwrap().insert(key, data_offset);

        Ok(())
    }

    pub async fn try_insert_slice<F, K>(&self, file: &mut F, key: K, value: &[u8]) -> io::Result<()>
    where
        F: AsyncWrite + AsyncSeek + AsyncSetLen + Unpin,
        K: AsRef<[u8]>,
    {
        self.try_insert(file, key, value.len() as u64).await?;
        file.write_all(value).await?;
        file.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use crate::AppendKV;
    use tempfile::{NamedTempFile, tempfile};
    use tokio::{
        fs::File,
        io::{AsyncSeekExt, AsyncWriteExt, BufWriter},
    };

    #[tokio::test]
    async fn round_trip() {
        let file = tempfile().unwrap();
        let mut file = File::from_std(file);

        let kv = AppendKV::open(&mut file).await.unwrap();
        kv.try_insert_slice(&mut file, b"meow", b"woof")
            .await
            .unwrap();
        assert_eq!(
            kv.try_insert_slice(&mut file, b"meow", b"yip")
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );

        let mut buffered = BufWriter::new(file);
        kv.insert_slice(&mut buffered, b"wisteria noise", b"woem")
            .await
            .unwrap();
        kv.insert_slice(&mut buffered, b"wisteria noise", b"meow")
            .await
            .unwrap();
        kv.insert(&mut buffered, b"empty", 6).await.unwrap();
        buffered.flush().await.unwrap();

        let mut file = buffered.into_inner();
        assert_eq!(kv.get_vec(&mut file, b"meow").await.unwrap(), b"woof");

        let kv = AppendKV::open(&mut file).await.unwrap();
        assert_eq!(
            kv.get_vec(&mut file, b"wisteria noise").await.unwrap(),
            b"meow"
        );
        assert_eq!(kv.get_vec(&mut file, b"meow").await.unwrap(), b"woof");
        assert_eq!(kv.get_vec(&mut file, b"empty").await.unwrap(), [0; 6]);
        assert_eq!(
            kv.get_vec(&mut file, b"mow").await.unwrap_err().kind(),
            io::ErrorKind::NotFound
        );

        assert_eq!(
            kv.inner().keys().collect::<Vec<_>>(),
            [
                b"empty".as_ref(),
                b"meow".as_ref(),
                b"wisteria noise".as_ref()
            ]
        );
    }

    #[tokio::test]
    async fn partial_concurrency() {
        let temp = NamedTempFile::new().unwrap();

        {
            let mut file1 = File::from_std(temp.reopen().unwrap());
            let kv = AppendKV::open(&mut file1).await.unwrap();
            kv.insert(&mut file1, b"one", 4).await.unwrap();
            let mut file2 = BufWriter::new(File::from_std(temp.reopen().unwrap()));
            kv.insert(&mut file2, b"two", 9).await.unwrap();
            let mut file3 = File::from_std(temp.reopen().unwrap());
            kv.insert(&mut file3, b"three", 6).await.unwrap();

            file3.write_all(b"squeak").await.unwrap();
            file2.write_all(b"boops you").await.unwrap();
            file1.write_all(b"meow").await.unwrap();

            file3.flush().await.unwrap();
            file1.flush().await.unwrap();
            file2.flush().await.unwrap();
        }

        let mut file = File::from_std(temp.into_file());
        let kv = AppendKV::open(&mut file).await.unwrap();
        assert_eq!(kv.get_vec(&mut file, b"one").await.unwrap(), b"meow");
        assert_eq!(kv.get_vec(&mut file, b"two").await.unwrap(), b"boops you");
        assert_eq!(kv.get_vec(&mut file, b"three").await.unwrap(), b"squeak");
    }

    #[tokio::test]
    async fn position() {
        let file = tempfile().unwrap();
        let mut file = File::from_std(file);
        let kv = AppendKV::open(&mut file).await.unwrap();

        kv.insert_slice(&mut file, "yip", b"yap").await.unwrap();
        kv.insert_slice(&mut file, "meow", b"woof").await.unwrap();

        assert_eq!(
            kv.get_pos(&mut file, "yip").await.unwrap().0,
            file.stream_position().await.unwrap()
        );
        assert_eq!(
            kv.get_pos(&mut file, "meow").await.unwrap().0,
            file.stream_position().await.unwrap()
        );
    }
}
