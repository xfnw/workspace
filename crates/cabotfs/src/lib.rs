// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

mod directory;
mod file_store;
mod fuse;

pub use directory::{Directory, DirectoryEntry, DirectoryEntryKind};
pub use file_store::FileStore;
pub use fuse::{CaFilesystem, mount};

#[derive(Debug, foxerror::FoxError)]
#[non_exhaustive]
pub enum Error {
    #[err(from)]
    Io(std::io::Error),
    ParseDirectory,
    #[err(from)]
    Timeout(tokio::time::error::Elapsed),
    Poisoned,
    Replaced,
    FileStore(Box<dyn std::error::Error + Send>),
}
