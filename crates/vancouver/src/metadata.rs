// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use crate::types::{Error, Version};
use serde::Deserialize;
use std::{fs::read_to_string, path::Path};

const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";

#[derive(Debug, Deserialize)]
struct CargoLock {
    package: Vec<CargoLockPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoLockPackage {
    name: String,
    version: String,
    source: Option<String>,
}

pub fn get_dependencies(lock_file: &Path) -> Result<Vec<(String, Version)>, Error> {
    let lock = read_to_string(lock_file).map_err(Error::LockOpen)?;
    let lock: CargoLock = toml_edit::de::from_str(&lock)?;
    Ok(lock
        .package
        .into_iter()
        .filter_map(|p| {
            p.source
                .is_some_and(|s| s == REGISTRY)
                .then(|| (p.name, Version::new(&p.version)))
        })
        .collect())
}
