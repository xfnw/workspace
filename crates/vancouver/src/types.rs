// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use std::fmt;

use serde::Serialize;

#[derive(Debug, foxerror::FoxError)]
pub enum Error {
    /// could not open lock file
    LockOpen(std::io::Error),
    /// could not open config file
    ConfigOpen(std::io::Error),
    /// could not write config
    ConfigWrite(std::io::Error),
    /// could not open audits file
    AuditsOpen(std::io::Error),
    /// could not write audits file
    AuditsWrite(std::io::Error),
    /// could not open merge source
    MergeSourceOpen(std::io::Error),
    /// could not deserialize toml
    #[err(from)]
    Deserialize(toml_edit::de::Error),
    /// could not find any dependencies
    ///
    /// this is probably a bug unless you actually have an empty lock
    /// file for some reason, please report it
    EmptyDependencies,
    /// could not parse delta
    ParseDelta(String),
    /// could not edit toml
    #[err(from)]
    Toml(toml_edit::TomlError),
    /// please do not the toml
    TomlBorked,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(Vec<String>);

impl Version {
    pub fn new(v: &str) -> Self {
        Self(v.split('.').map(str::to_string).collect())
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut i = self.0.iter();
        if let Some(p) = i.next() {
            write!(f, "{p}")?;
            for p in i {
                write!(f, ".{p}")?;
            }
        }
        Ok(())
    }
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
