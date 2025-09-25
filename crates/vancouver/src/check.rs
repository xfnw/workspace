// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use crate::types::Error;
use rayon::prelude::*;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::read_to_string,
    process::ExitCode,
};

#[derive(Debug, Deserialize)]
struct Config {
    /// extra criteria that overrides criteria specified in the audits
    /// file
    #[serde(default)]
    criteria: BTreeMap<String, Criteria>,
    /// policy for crates without a policy specified
    #[serde(default, alias = "default-policy")]
    default_policy: Policy,
    #[serde(default)]
    policy: BTreeMap<String, Policy>,
    #[serde(default)]
    exempt: BTreeMap<String, BTreeSet<Audit>>,
}

#[derive(Debug, Deserialize, Default)]
struct Policy {
    #[serde(default, alias = "require-all")]
    require_all: Option<BTreeSet<String>>,
}

#[derive(Debug, Deserialize)]
struct Audits {
    /// criteria that can be overridden by the config file
    #[serde(default)]
    criteria: BTreeMap<String, Criteria>,
    audits: BTreeMap<String, BTreeSet<Audit>>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct Audit {
    /// the criteria audited for
    criteria: String,
    /// the version range for an audit of the changes between two
    /// versions
    #[serde(default)]
    delta: Option<String>,
    /// the version for a standalone audit of an entire version
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Criteria {
    /// give the listed criteria to everything that has this criteria
    #[serde(default)]
    implies: BTreeSet<String>,
    /// automatically imply this criteria on everything that has all
    /// of the listed criteria
    #[serde(default)]
    implied_all: BTreeSet<String>,
    /// automatically imply this criteria on everything that has any
    /// of the listed criteria
    #[serde(default)]
    implied_any: BTreeSet<String>,
}

pub fn do_check(args: &crate::CheckArgs) -> Result<ExitCode, Error> {
    let dependencies = crate::metadata::get_dependencies(&args.lock)?;
    if dependencies.is_empty() {
        return Err(Error::EmptyDependencies);
    }

    let config = read_to_string(&args.config).map_err(Error::ConfigOpen)?;
    let config: Config = toml_edit::de::from_str(&config)?;
    let audits = read_to_string(&args.audits).map_err(Error::ConfigOpen)?;
    let audits: Audits = toml_edit::de::from_str(&audits)?;

    let unaudited: Vec<_> = dependencies
        .into_par_iter()
        .flat_map(|(_name, _version)| Some(1))
        .collect();

    if unaudited.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }

    for c in unaudited {
        println!("oh no {c}");
    }

    Ok(ExitCode::FAILURE)
}
