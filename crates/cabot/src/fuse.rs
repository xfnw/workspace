// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use crate::{Error, directory, file_store::FileStore, tohex_digest, unhex_digest};
use append_only_vec::AppendOnlyVec;
use fuse3::{
    Inode, MountOptions,
    raw::{MountHandle, prelude::*},
};
use std::{
    ffi::{OsStr, OsString},
    num::NonZeroU32,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, UNIX_EPOCH},
};
use tokio::sync::RwLock;
use zerocopy::{Immutable, IntoBytes};

const TTL: Duration = Duration::from_secs(1);

struct CaInner {
    store: FileStore,
    inodes: AppendOnlyVec<Entry>,
    realize_timeout: Duration,
    poisoned: AtomicBool,
}

#[derive(Clone)]
pub struct CaFilesystem(Arc<CaInner>);

impl CaFilesystem {
    pub fn new(store: FileStore, resume: Option<[u8; 16]>, timeout: u64) -> Self {
        let inodes = AppendOnlyVec::new();
        inodes.push(Entry {
            parent: 1,
            data: DataKind::Directory(RwLock::new(if let Some(hash) = resume {
                DataStatus::Placeholder { hash }
            } else {
                DataStatus::Dirty { body: vec![] }
            })),
        });
        Self(Arc::new(CaInner {
            store,
            inodes,
            realize_timeout: Duration::from_secs(timeout),
            poisoned: AtomicBool::new(false),
        }))
    }

    fn get(&self, inode: Inode) -> &Entry {
        #[expect(clippy::cast_possible_truncation)]
        &self.0.inodes[inode as usize - 1]
    }

    fn push(&self, entry: Entry) -> Result<Inode, Error> {
        if self.0.poisoned.load(Ordering::Relaxed) {
            return Err(Error::Poisoned);
        }
        Ok(self.0.inodes.push(entry) as Inode + 1)
    }

    async fn sync_inode(&self, inode: Inode) -> Result<[u8; 16], Error> {
        self.get(inode)
            .sync(self)
            .await
            .inspect_err(|_| self.poison())
    }

    pub async fn sync(&self) -> Result<[u8; 16], Error> {
        self.sync_inode(1).await
    }

    async fn realize(&self, inode: Inode) -> Result<(), Error> {
        let fs = self.clone();
        let task = tokio::spawn(async move { fs.get(inode).realize(&fs, inode).await });
        tokio::time::timeout(self.0.realize_timeout, task)
            .await?
            .unwrap()
    }

    async fn attr(&self, req: Request, inode: Inode) -> Result<FileAttr, Error> {
        self.realize(inode).await?;
        let data = &self.get(inode).data;
        let len = match data {
            DataKind::File(lock) => lock.read().await.get().unwrap().len(),
            DataKind::Directory(lock) => lock.read().await.get().unwrap().len(),
        } as u64;
        Ok(FileAttr {
            ino: inode,
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
                DataKind::File(_) => 0o644,
                DataKind::Directory(_) => 0o755,
            },
            nlink: match data {
                DataKind::File(_) => 1,
                DataKind::Directory(_) => 3,
            },
            uid: req.uid,
            gid: req.gid,
            rdev: 0,
            blksize: 4096,
        })
    }

    pub fn poison(&self) {
        self.0.poisoned.store(true, Ordering::Relaxed);
    }
}

impl Filesystem for CaFilesystem {
    async fn init(&self, _req: Request) -> fuse3::Result<ReplyInit> {
        Ok(ReplyInit {
            max_write: NonZeroU32::new(16 * 1024).unwrap(),
        })
    }

    async fn destroy(&self, _req: Request) {
        if let Ok(hash) = self.sync().await {
            use std::io::{Write, stdout};
            let mut p = stdout();
            p.write_all(&tohex_digest(hash)).unwrap();
            p.write_all(b"\n").unwrap();
        }

        _ = self.0.store.shutdown().await;
    }

    async fn create(
        &self,
        req: Request,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _flags: u32,
    ) -> fuse3::Result<ReplyCreated> {
        if name.len() >= 4096 {
            return Err(libc::ENAMETOOLONG.into());
        }
        let entry = self.get(parent);
        let DataKind::Directory(parent_data) = &entry.data else {
            return Err(libc::ENOTDIR.into());
        };
        self.realize(parent).await.map_err(|_| libc::EIO)?;
        entry.mark_dirty(self).await?;
        let mut parent_data = parent_data.write().await;
        let parent_entries = parent_data.get_mut_dirty().ok_or(libc::EBUSY)?;
        if parent_entries.iter().any(|e| e.name == name) {
            return Err(libc::EEXIST.into());
        }
        let inode = self
            .push(Entry {
                parent,
                data: DataKind::File(RwLock::new(DataStatus::Dirty { body: vec![] })),
            })
            .map_err(|_| libc::EROFS)?;
        parent_entries.push(DirEntry {
            name: name.to_os_string(),
            inode,
        });
        Ok(ReplyCreated {
            ttl: TTL,
            attr: self.attr(req, inode).await.map_err(|_| libc::EIO)?,
            generation: 0,
            fh: 0,
            // no idea what is supposed to go on here.
            // returning the same flags we were passed causes EIO
            // upon opening files with O_EXCL
            flags: 0,
        })
    }

    async fn lookup(&self, req: Request, parent: u64, name: &OsStr) -> fuse3::Result<ReplyEntry> {
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
            attr: self.attr(req, inode).await.map_err(|_| libc::EIO)?,
            generation: 0,
        })
    }

    async fn getattr(
        &self,
        req: Request,
        inode: Inode,
        _fh: Option<u64>,
        _flags: u32,
    ) -> fuse3::Result<ReplyAttr> {
        Ok(ReplyAttr {
            ttl: TTL,
            attr: self.attr(req, inode).await.map_err(|_| libc::EIO)?,
        })
    }

    async fn setattr(
        &self,
        req: Request,
        inode: Inode,
        fh: Option<u64>,
        set_attr: SetAttr,
    ) -> fuse3::Result<ReplyAttr> {
        let entry = self.get(inode);
        self.realize(inode).await.map_err(|_| libc::EIO)?;
        entry.mark_dirty(self).await?;

        if let Some(new_len) = set_attr.size {
            let DataKind::File(lock) = &entry.data else {
                return Err(libc::EISDIR.into());
            };
            let mut data = lock.write().await;
            #[expect(clippy::cast_possible_truncation)]
            data.get_mut_dirty()
                .ok_or(libc::EBUSY)?
                .truncate(new_len as usize);
        }

        self.getattr(req, inode, fh, 0).await
    }

    async fn mkdir(
        &self,
        req: Request,
        parent: Inode,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
    ) -> fuse3::Result<ReplyEntry> {
        if name.len() >= 4096 {
            return Err(libc::ENAMETOOLONG.into());
        }
        let parent_entry = self.get(parent);
        let DataKind::Directory(parent_data) = &parent_entry.data else {
            return Err(libc::ENOTDIR.into());
        };
        self.realize(parent).await.map_err(|_| libc::EIO)?;
        parent_entry.mark_dirty(self).await?;
        let mut parent_data = parent_data.write().await;
        let parent_entries = parent_data.get_mut_dirty().ok_or(libc::EBUSY)?;
        if parent_entries.iter().any(|e| e.name == name) {
            return Err(libc::EEXIST.into());
        }
        let inode = self
            .push(Entry {
                parent,
                data: DataKind::Directory(RwLock::new(DataStatus::Dirty { body: vec![] })),
            })
            .map_err(|_| libc::EROFS)?;
        parent_entries.push(DirEntry {
            name: name.to_os_string(),
            inode,
        });
        Ok(ReplyEntry {
            ttl: TTL,
            attr: self.attr(req, inode).await.map_err(|_| libc::EIO)?,
            generation: 0,
        })
    }

    async fn unlink(&self, _req: Request, parent: Inode, name: &OsStr) -> fuse3::Result<()> {
        let parent_entry = self.get(parent);
        let DataKind::Directory(parent_data) = &parent_entry.data else {
            return Err(libc::ENOTDIR.into());
        };
        self.realize(parent).await.map_err(|_| libc::EIO)?;
        parent_entry.mark_dirty(self).await?;
        let mut parent_data = parent_data.write().await;
        let parent_entries = parent_data.get_mut_dirty().ok_or(libc::EBUSY)?;
        let mut anything_deleted = false;

        // naughtily skip checking if the thing we're unlinking is a directory,
        // since linux (or maybe the rm implementation im using? idk?) already
        // checks that for us, and we can just reuse this function for rmdir too
        parent_entries.retain(|e| {
            if e.name == name {
                false
            } else {
                anything_deleted = true;
                true
            }
        });

        if anything_deleted {
            Ok(())
        } else {
            Err(libc::ENOENT.into())
        }
    }

    async fn rmdir(&self, req: Request, parent: Inode, name: &OsStr) -> fuse3::Result<()> {
        self.unlink(req, parent, name).await
    }

    async fn rename(
        &self,
        _req: Request,
        parent: Inode,
        name: &OsStr,
        new_parent: Inode,
        new_name: &OsStr,
    ) -> fuse3::Result<()> {
        if parent != new_parent {
            return Err(libc::ENOSYS.into());
        }
        if new_name.len() >= 4096 {
            return Err(libc::ENAMETOOLONG.into());
        }

        let parent_entry = self.get(parent);
        let DataKind::Directory(parent_data) = &parent_entry.data else {
            return Err(libc::ENOTDIR.into());
        };
        self.realize(parent).await.map_err(|_| libc::EIO)?;
        parent_entry.mark_dirty(self).await?;
        let mut parent_data = parent_data.write().await;
        let parent_entries = parent_data.get_mut_dirty().ok_or(libc::EBUSY)?;

        if !parent_entries.iter().any(|e| e.name == name) {
            return Err(libc::ENOENT.into());
        }

        parent_entries.retain(|e| e.name != new_name);

        for entry in parent_entries {
            if entry.name == name {
                entry.name = new_name.to_os_string();
            }
        }

        Ok(())
    }

    #[expect(clippy::cast_possible_truncation)]
    async fn read(
        &self,
        _req: Request,
        inode: Inode,
        _fh: u64,
        offset: u64,
        size: u32,
    ) -> fuse3::Result<ReplyData> {
        let DataKind::File(data) = &self.get(inode).data else {
            return Err(libc::EISDIR.into());
        };
        self.realize(inode).await.map_err(|_| libc::EIO)?;
        let data = data.read().await;
        let data = data.get().unwrap();

        Ok(ReplyData {
            data: data[offset as usize..std::cmp::min(offset as usize + size as usize, data.len())]
                .to_vec()
                .into(),
        })
    }

    #[expect(clippy::cast_possible_truncation)]
    async fn write(
        &self,
        _req: Request,
        inode: Inode,
        _fh: u64,
        offset: u64,
        data: &[u8],
        _write_flags: u32,
        _flags: u32,
    ) -> fuse3::Result<ReplyWrite> {
        let entry = self.get(inode);
        let DataKind::File(file) = &entry.data else {
            return Err(libc::EISDIR.into());
        };
        self.realize(inode).await.map_err(|_| libc::EIO)?;
        entry.mark_dirty(self).await?;
        let mut file = file.write().await;
        let file = file.get_mut_dirty().ok_or(libc::EBUSY)?;

        if file.len() < data.len() + offset as usize {
            file.resize(data.len() + offset as usize, 0);
        }

        file[offset as usize..data.len() + offset as usize].copy_from_slice(data);

        Ok(ReplyWrite {
            written: data.len() as u32,
        })
    }

    #[expect(clippy::cast_possible_truncation)]
    #[expect(clippy::cast_possible_wrap)]
    async fn readdirplus(
        &self,
        req: Request,
        // why is this called parent in the trait?
        inode: Inode,
        _fh: u64,
        offset: u64,
        _lock_owner: u64,
    ) -> fuse3::Result<
        ReplyDirectoryPlus<
            impl futures_util::Stream<Item = fuse3::Result<DirectoryEntryPlus>> + Send,
        >,
    > {
        let entry = self.get(inode);
        let DataKind::Directory(data) = &entry.data else {
            return Err(libc::ENOTDIR.into());
        };

        let mut entries = vec![
            DirectoryEntryPlus {
                inode,
                generation: 0,
                kind: FileType::Directory,
                name: OsString::from("."),
                offset: 1,
                attr: self.attr(req, inode).await.map_err(|_| libc::EIO)?,
                entry_ttl: TTL,
                attr_ttl: TTL,
            },
            DirectoryEntryPlus {
                inode: entry.parent,
                generation: 0,
                kind: FileType::Directory,
                name: OsString::from(".."),
                offset: 2,
                attr: self.attr(req, entry.parent).await.map_err(|_| libc::EIO)?,
                entry_ttl: TTL,
                attr_ttl: TTL,
            },
        ];

        for (i, DirEntry { name, inode }) in data.read().await.get().unwrap().iter().enumerate() {
            entries.push(DirectoryEntryPlus {
                inode: *inode,
                generation: 0,
                kind: match self.get(*inode).data {
                    DataKind::File(_) => FileType::RegularFile,
                    DataKind::Directory(_) => FileType::Directory,
                },
                name: name.clone(),
                offset: i as i64 + 3,
                attr: self.attr(req, *inode).await.map_err(|_| libc::EIO)?,
                entry_ttl: TTL,
                attr_ttl: TTL,
            });
        }

        Ok(ReplyDirectoryPlus {
            entries: futures_util::stream::iter(entries.into_iter().skip(offset as usize).map(Ok)),
        })
    }

    async fn setxattr(
        &self,
        _req: Request,
        inode: Inode,
        name: &OsStr,
        value: &[u8],
        _flags: u32,
        _position: u32,
    ) -> fuse3::Result<()> {
        if name.as_encoded_bytes() != b"user.hash" {
            return Err(libc::ENODATA.into());
        }
        let hash = unhex_digest(value).ok_or(libc::EINVAL)?;
        let entry = self.get(inode);
        let parent = self.get(entry.parent);
        parent.mark_dirty(self).await?;
        match &entry.data {
            DataKind::File(lock) => *lock.write().await = DataStatus::Placeholder { hash },
            DataKind::Directory(lock) => *lock.write().await = DataStatus::Placeholder { hash },
        }
        Ok(())
    }

    async fn getxattr(
        &self,
        _req: Request,
        inode: Inode,
        name: &OsStr,
        size: u32,
    ) -> fuse3::Result<ReplyXAttr> {
        if name.as_encoded_bytes() != b"user.hash" {
            return Err(libc::ENODATA.into());
        }
        if size == 0 {
            // FIXME: remove after fuse3 > 0.9.0
            // workaround for https://github.com/Sherlock-Holo/fuse3/issues/130
            #[derive(IntoBytes, Immutable)]
            #[repr(C)]
            struct fuse_getxattr_out {
                size: u32,
                _padding: u32,
            }
            return Ok(ReplyXAttr::Data(
                fuse_getxattr_out {
                    size: 32,
                    _padding: 0,
                }
                .as_bytes()
                .into(),
            ));
            //return Ok(ReplyXAttr::Size(32));
        }
        if size < 32 {
            return Err(libc::ERANGE.into());
        }
        let hash = self.sync_inode(inode).await.map_err(|_| libc::EIO)?;
        Ok(ReplyXAttr::Data(tohex_digest(hash).to_vec().into()))
    }

    #[expect(clippy::cast_possible_truncation)]
    async fn listxattr(
        &self,
        _req: Request,
        _inode: Inode,
        size: u32,
    ) -> fuse3::Result<ReplyXAttr> {
        const XATTR_LIST: &[u8] = b"user.hash\0";
        if size == 0 {
            return Ok(ReplyXAttr::Size(XATTR_LIST.len() as u32));
        }
        if size < XATTR_LIST.len() as u32 {
            return Err(libc::ERANGE.into());
        }
        Ok(ReplyXAttr::Data(XATTR_LIST.to_vec().into()))
    }
}

struct Entry {
    parent: Inode,
    data: DataKind,
}

impl Entry {
    async fn sync(&self, fs: &CaFilesystem) -> Result<[u8; 16], Error> {
        match &self.data {
            DataKind::File(status) => status.write().await.sync(fs).await,
            DataKind::Directory(status) => status.write().await.sync(fs).await,
        }
    }

    async fn realize(&self, fs: &CaFilesystem, my_inode: Inode) -> Result<(), Error> {
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

        let serialized =
            fs.0.store
                .retrieve(placeholder_hash)
                .await
                .inspect_err(|e| {
                    if matches!(e, Error::Io(_)) {
                        fs.poison();
                    }
                })?;

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
                    })?;
                    entries.push(DirEntry { name, inode });
                }

                lock.write().await.overwrite_placeholder(entries);
            }
        }

        Ok(())
    }

    async fn mark_dirty(&self, fs: &CaFilesystem) -> fuse3::Result<()> {
        if fs.0.poisoned.load(Ordering::Relaxed) {
            return Err(libc::EROFS.into());
        }

        if match &self.data {
            DataKind::File(lock) => lock.write().await.unpropagated_mark_dirty(),
            DataKind::Directory(lock) => lock.write().await.unpropagated_mark_dirty(),
        }
        .ok_or(libc::EBUSY)?
        {
            Box::pin(fs.get(self.parent).mark_dirty(fs)).await?;
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
        *self = old;
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

    fn unpropagated_mark_dirty(&mut self) -> Option<bool> {
        match self {
            Self::Placeholder { .. } => None,
            Self::Clean { .. } => {
                let old = std::mem::replace(self, Self::Placeholder { hash: [0; 16] });
                let Self::Clean { body, .. } = old else {
                    unreachable!();
                };
                *self = Self::Dirty { body };
                Some(true)
            }
            Self::Dirty { .. } => Some(false),
        }
    }

    fn get_mut_dirty(&mut self) -> Option<&mut Vec<T>> {
        match self {
            Self::Dirty { body } => Some(body),
            _ => None,
        }
    }
}

impl DataStatus<u8> {
    async fn sync(&mut self, fs: &CaFilesystem) -> Result<[u8; 16], Error> {
        match self {
            Self::Placeholder { hash } | Self::Clean { hash, .. } => Ok(*hash),
            Self::Dirty { body } => {
                let newhash = fs.0.store.store(body).await?;
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
                    let hash = Box::pin(fs.sync_inode(*inode)).await?;
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

                dir.entries.sort_by(|a, b| {
                    a.name
                        .cmp(&b.name)
                        .then_with(|| a.hash.cmp(&b.hash))
                        .then_with(|| a.kind.cmp(&b.kind))
                });

                let newhash = fs.0.store.store(&dir.serialize()).await?;
                self.dirty_add_hash(newhash);
                Ok(newhash)
            }
        }
    }
}

struct DirEntry {
    name: OsString,
    inode: Inode,
}

pub async fn mount(fs: CaFilesystem, mount_path: &Path) -> MountHandle {
    let mut mount_options = MountOptions::default();
    mount_options
        .fs_name("cabotfs".to_string())
        .no_open_support(true)
        .no_open_dir_support(true)
        .force_readdir_plus(true);

    Session::new(mount_options)
        .mount_with_unprivileged(fs, mount_path)
        .await
        .unwrap()
}
