// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use crate::types::{Error, Version};
use serde::Deserialize;
use std::{path::Path, process::Stdio};

const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoMetadataPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadataPackage {
    name: String,
    version: String,
    source: Option<String>,
}

pub fn get_dependencies(manifest: Option<&Path>) -> Result<Vec<(String, Version)>, Error> {
    let program = std::env::var("CARGO");
    let program = program.as_deref().unwrap_or("cargo");
    let mut command = std::process::Command::new(program);

    command
        .arg("metadata")
        .arg("--format-version=1")
        .arg("--all-features")
        .arg("--frozen");

    if let Some(manifest) = manifest {
        command.arg("--manifest-path").arg(manifest);
    }

    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .map_err(Error::MetadataCommand)?;

    if !output.status.success() {
        return Err(Error::MetadataExit(output.status));
    }

    let lock: CargoMetadata = serde_json::de::from_slice(&output.stdout)?;

    Ok(lock
        .packages
        .into_iter()
        .filter_map(|p| {
            p.source
                .is_some_and(|s| s == REGISTRY)
                .then(|| (p.name, Version::new(&p.version)))
        })
        .collect())
}
