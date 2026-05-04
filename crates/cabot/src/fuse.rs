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
use std::{
    ffi::{OsStr, OsString},
    num::NonZeroU32,
    path::Path,
    sync::Arc,
    time::UNIX_EPOCH,
};
use tokio::sync::RwLock;

const TTL: std::time::Duration = std::time::Duration::from_secs(1);

pub struct CaFilesystem {
    store: Arc<FileStore>,
    inodes: AppendOnlyVec<Entry>,
}

impl CaFilesystem {
    pub fn new(store: Arc<FileStore>, resume: Option<[u8; 16]>) -> Self {
        let inodes = AppendOnlyVec::new();
        inodes.push(Entry {
            parent: 1,
            data: DataKind::Directory(RwLock::new(if let Some(hash) = resume {
                DataStatus::Placeholder { hash }
            } else {
                DataStatus::Dirty { body: vec![] }
            })),
        });
        Self { store, inodes }
    }

    fn get(&self, inode: usize) -> &Entry {
        &self.inodes[inode - 1]
    }

    fn push(&self, entry: Entry) -> usize {
        self.inodes.push(entry) + 1
    }

    async fn sync(&self, inode: usize) -> Result<[u8; 16], Error> {
        self.get(inode).sync(self).await
    }

    async fn realize(&self, inode: usize) -> Result<(), Error> {
        self.get(inode).realize(self, inode).await
    }

    async fn attr(&self, inode: usize) -> Result<FileAttr, Error> {
        self.realize(inode).await?;
        let data = &self.get(inode).data;
        let len = match data {
            DataKind::File(lock) => lock.read().await.get().unwrap().len(),
            DataKind::Directory(lock) => lock.read().await.get().unwrap().len(),
        } as u64;
        Ok(FileAttr {
            ino: inode as u64,
            size: len,
            blocks: len.div_ceil(4096),
            atime: UNIX_EPOCH.into(),
            mtime: UNIX_EPOCH.into(),
            ctime: UNIX_EPOCH.into(),
            kind: match data {
                DataKind::File(_) => FileType::RegularFile,
                DataKind::Directory(_) => FileType::Directory,
            },
            perm: match data {
                DataKind::File(_) => 0o666,
                DataKind::Directory(_) => 0o777,
            },
            nlink: match data {
                DataKind::File(_) => 1,
                DataKind::Directory(_) => 3,
            },
            uid: 0,
            gid: 0,
            rdev: 0,
            blksize: 4096,
        })
    }
}

impl Filesystem for CaFilesystem {
    async fn init(&self, _req: Request) -> fuse3::Result<ReplyInit> {
        Ok(ReplyInit {
            max_write: NonZeroU32::new(16 * 1024).unwrap(),
        })
    }

    async fn destroy(&self, _req: Request) {
        if let Ok(hash) = self.sync(1).await {
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

    async fn create(
        &self,
        _req: Request,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        flags: u32,
    ) -> fuse3::Result<ReplyCreated> {
        let parent = parent as usize;
        let DataKind::Directory(parent_data) = &self.get(parent).data else {
            return Err(libc::ENOTDIR.into());
        };
        self.realize(parent).await.map_err(|_| libc::EIO)?;
        let mut parent_data = parent_data.write().await;
        let parent_entries = parent_data.get_mut().unwrap();
        if parent_entries.iter().any(|e| e.name == name) {
            return Err(libc::EEXIST.into());
        }
        let inode = self.push(Entry {
            parent,
            data: DataKind::File(RwLock::new(DataStatus::Dirty { body: vec![] })),
        });
        parent_entries.push(DirEntry {
            name: name.to_os_string(),
            inode,
        });
        Ok(ReplyCreated {
            ttl: TTL,
            attr: self.attr(inode).await.map_err(|_| libc::EIO)?,
            generation: 0,
            fh: 0,
            flags,
        })
    }

    async fn lookup(&self, _req: Request, parent: u64, name: &OsStr) -> fuse3::Result<ReplyEntry> {
        let parent = parent as usize;
        let DataKind::Directory(parent_data) = &self.get(parent).data else {
            return Err(libc::ENOTDIR.into());
        };
        self.realize(parent).await.map_err(|_| libc::EIO)?;
        let parent_data = parent_data.read().await;
        let parent_entries = parent_data.get().unwrap();
        let Some(inode) = parent_entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.inode)
        else {
            return Err(libc::ENOENT.into());
        };

        Ok(ReplyEntry {
            ttl: TTL,
            attr: self.attr(inode).await.map_err(|_| libc::EIO)?,
            generation: 0,
        })
    }
}

struct Entry {
    parent: usize,
    data: DataKind,
}

impl Entry {
    async fn sync(&self, fs: &CaFilesystem) -> Result<[u8; 16], Error> {
        match &self.data {
            DataKind::File(status) => status.write().await.sync(fs).await,
            DataKind::Directory(status) => status.write().await.sync(fs).await,
        }
    }

    async fn realize(&self, fs: &CaFilesystem, my_inode: usize) -> Result<(), Error> {
        let placeholder_hash = match &self.data {
            DataKind::File(lock) => match *lock.read().await {
                DataStatus::Placeholder { hash } => hash,
                DataStatus::Clean { .. } | DataStatus::Dirty { .. } => return Ok(()),
            },
            DataKind::Directory(lock) => match *lock.read().await {
                DataStatus::Placeholder { hash } => hash,
                DataStatus::Clean { .. } | DataStatus::Dirty { .. } => return Ok(()),
            },
        };

        let serialized = fs.store.retrieve(placeholder_hash).await?;

        match &self.data {
            DataKind::File(lock) => lock.write().await.overwrite_placeholder(serialized),
            DataKind::Directory(lock) => {
                let dir = directory::Directory::parse(&serialized).ok_or(Error::ParseDirectory)?;
                let mut entries = vec![];

                for directory::DirectoryEntry { hash, kind, name } in dir.entries {
                    let inode = fs.push(Self {
                        parent: my_inode,
                        data: match kind {
                            directory::DirectoryEntryKind::File => {
                                DataKind::File(RwLock::new(DataStatus::Placeholder { hash }))
                            }
                            directory::DirectoryEntryKind::Subdirectory => {
                                DataKind::Directory(RwLock::new(DataStatus::Placeholder { hash }))
                            }
                        },
                    });
                    entries.push(DirEntry { name, inode });
                }

                lock.write().await.overwrite_placeholder(entries);
            }
        }

        Ok(())
    }
}

enum DataKind {
    File(RwLock<DataStatus<u8>>),
    Directory(RwLock<DataStatus<DirEntry>>),
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

    fn overwrite_placeholder(&mut self, body: Vec<T>) {
        if let Self::Placeholder { hash } = self {
            let hash = *hash;
            *self = Self::Clean { hash, body };
        }
    }

    fn get(&self) -> Option<&[T]> {
        match self {
            Self::Placeholder { .. } => None,
            Self::Clean { body, .. } | Self::Dirty { body } => Some(body),
        }
    }

    fn get_mut(&mut self) -> Option<&mut Vec<T>> {
        match self {
            Self::Placeholder { .. } => None,
            Self::Clean { .. } => {
                let old = std::mem::replace(self, Self::Placeholder { hash: [0; 16] });
                let Self::Clean { body, .. } = old else {
                    unreachable!();
                };
                _ = std::mem::replace(self, Self::Dirty { body });
                let Self::Dirty { body, .. } = self else {
                    unreachable!();
                };
                Some(body)
            }
            Self::Dirty { body } => Some(body),
        }
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
                    let kind = match fs.get(*inode).data {
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
    mount_options
        .fs_name("cabotfs".to_string())
        .no_open_support(true)
        .no_open_dir_support(true);

    Session::new(mount_options)
        .mount_with_unprivileged(fs, mount_path)
        .await
        .unwrap()
}
