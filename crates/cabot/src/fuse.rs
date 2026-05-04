// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

#![allow(clippy::cast_possible_truncation)]

use crate::{Error, directory, file_store::FileStore, tohex_digest};
use append_only_vec::AppendOnlyVec;
use fuse3::{
    MountOptions,
    raw::{MountHandle, prelude::*},
};
use std::{ffi::OsString, num::NonZeroU32, path::Path, sync::Arc};
use tokio::sync::RwLock;

pub struct CaFilesystem {
    store: Arc<FileStore>,
    inodes: AppendOnlyVec<RwLock<Entry>>,
}

impl CaFilesystem {
    pub fn new(store: Arc<FileStore>, resume: Option<[u8; 16]>) -> Self {
        let inodes = AppendOnlyVec::new();
        inodes.push(RwLock::new(Entry {
            parent: 0,
            data: DataKind::Directory(if let Some(hash) = resume {
                DataStatus::Placeholder { hash }
            } else {
                DataStatus::Dirty { body: vec![] }
            }),
        }));
        Self { store, inodes }
    }

    async fn sync(&self, inode: usize) -> Result<[u8; 16], Error> {
        self.inodes[inode].write().await.sync(self).await
    }
}

impl Filesystem for CaFilesystem {
    async fn init(&self, _req: Request) -> fuse3::Result<ReplyInit> {
        Ok(ReplyInit {
            max_write: NonZeroU32::new(16 * 1024).unwrap(),
        })
    }

    async fn destroy(&self, _req: Request) {
        if let Ok(hash) = self.sync(0).await {
            use std::io::{Write, stdout};
            let mut p = stdout();
            p.write_all(&tohex_digest(hash)).unwrap();
            p.write_all(b"\n").unwrap();
        }

        _ = self.store.shutdown().await;
    }

    async fn fsync(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        _datasync: bool,
    ) -> fuse3::Result<()> {
        if self.sync(inode as usize).await.is_err() {
            return Err(libc::EIO.into());
        }
        Ok(())
    }

    async fn fsyncdir(
        &self,
        _req: Request,
        inode: u64,
        _fh: u64,
        _datasync: bool,
    ) -> fuse3::Result<()> {
        if self.sync(inode as usize).await.is_err() {
            return Err(libc::EIO.into());
        }
        Ok(())
    }
}

struct Entry {
    parent: usize,
    data: DataKind,
}

impl Entry {
    async fn sync(&mut self, fs: &CaFilesystem) -> Result<[u8; 16], Error> {
        match &mut self.data {
            DataKind::File(status) => status.sync(fs).await,
            DataKind::Directory(status) => status.sync(fs).await,
        }
    }
}

enum DataKind {
    File(DataStatus<u8>),
    Directory(DataStatus<DirEntry>),
}

enum DataStatus<T> {
    Placeholder { hash: [u8; 16] },
    Clean { hash: [u8; 16], body: Vec<T> },
    Dirty { body: Vec<T> },
}

impl<T> DataStatus<T> {
    fn dirty_add_hash(&mut self, hash: [u8; 16]) {
        // naughty temporary nonsense value.
        // would be more efficient to transmute to a MaybeUninit and then replace
        // it with uninitialized data, but this is not exactly performance
        // sensitive so it is not worth it
        let old = std::mem::replace(self, Self::Placeholder { hash: [0; 16] });

        if let Self::Dirty { body } = old {
            *self = Self::Clean { hash, body };
            return;
        }

        // leave stuff in a somewhat reasonable state just in case our panic gets caught
        _ = std::mem::replace(self, old);
        panic!("tried to add a hash to non-dirty data");
    }
}

impl DataStatus<u8> {
    async fn sync(&mut self, fs: &CaFilesystem) -> Result<[u8; 16], Error> {
        match self {
            Self::Placeholder { hash } | Self::Clean { hash, .. } => Ok(*hash),
            Self::Dirty { body } => {
                let newhash = fs.store.store(body).await?;
                self.dirty_add_hash(newhash);
                Ok(newhash)
            }
        }
    }
}

impl DataStatus<DirEntry> {
    async fn sync(&mut self, fs: &CaFilesystem) -> Result<[u8; 16], Error> {
        match self {
            Self::Placeholder { hash } | Self::Clean { hash, .. } => Ok(*hash),
            Self::Dirty { body } => {
                let mut dir = directory::Directory { entries: vec![] };

                for DirEntry { name, inode } in body.iter() {
                    let hash = Box::pin(fs.sync(*inode)).await?;
                    let kind = match fs.inodes[*inode].read().await.data {
                        DataKind::File(_) => directory::DirectoryEntryKind::File,
                        DataKind::Directory(_) => directory::DirectoryEntryKind::Subdirectory,
                    };
                    dir.entries.push(directory::DirectoryEntry {
                        hash,
                        kind,
                        name: name.clone(),
                    });
                }

                let newhash = fs.store.store(&dir.serialize()).await?;
                self.dirty_add_hash(newhash);
                Ok(newhash)
            }
        }
    }
}

struct DirEntry {
    name: OsString,
    inode: usize,
}

pub async fn mount(fs: CaFilesystem, mount_path: &Path) -> MountHandle {
    let mut mount_options = MountOptions::default();
    mount_options.fs_name("cabotfs".to_string());

    Session::new(mount_options)
        .mount_with_unprivileged(fs, mount_path)
        .await
        .unwrap()
}
