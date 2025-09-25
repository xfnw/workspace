// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use crate::types::{Error, Version};
use serde::Deserialize;
use std::{fs::read_to_string, path::Path};

#[derive(Debug, Deserialize)]
struct CargoLock {
    package: Vec<CargoLockPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoLockPackage {
    name: String,
    version: String,
}

pub fn get_dependencies(lock_file: &Path) -> Result<Vec<(String, Version)>, Error> {
    let lock = read_to_string(lock_file).map_err(Error::LockOpen)?;
    let lock: CargoLock = toml_edit::de::from_str(&lock)?;
    Ok(lock
        .package
        .into_iter()
        .map(|p| (p.name, Version::new(&p.version)))
        .collect())
}
