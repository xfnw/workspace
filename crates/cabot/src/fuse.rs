// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use crate::file_store::FileStore;
use fuse3::{
    MountOptions,
    raw::{MountHandle, prelude::*},
};
use std::{num::NonZeroU32, path::Path, sync::Arc};

pub struct CaFilesystem {
    store: Arc<FileStore>,
}

impl CaFilesystem {
    pub fn new(store: Arc<FileStore>) -> Self {
        Self { store }
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

pub async fn mount(fs: CaFilesystem, mount_path: &Path) -> MountHandle {
    let mut mount_options = MountOptions::default();
    mount_options.fs_name("cabotfs".to_string());

    Session::new(mount_options)
        .mount_with_unprivileged(fs, mount_path)
        .await
        .unwrap()
}
