// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use crate::file_store::FileStore;
use append_only_vec::AppendOnlyVec;
use fuse3::{
    MountOptions,
    raw::{MountHandle, prelude::*},
};
use std::{
    ffi::OsString,
    num::NonZeroU32,
    path::Path,
    sync::{Arc, RwLock},
};

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
}

impl Filesystem for CaFilesystem {
    async fn init(&self, _req: Request) -> fuse3::Result<ReplyInit> {
        Ok(ReplyInit {
            max_write: NonZeroU32::new(16 * 1024).unwrap(),
        })
    }

    async fn destroy(&self, _req: Request) {
        _ = self.store.shutdown().await;
    }
}

struct Entry {
    parent: usize,
    data: DataKind,
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
