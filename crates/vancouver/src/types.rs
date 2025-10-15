// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use std::{cmp::Ordering, fmt};

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
    /// the fail and base options are mutually exclusive
    FailAndBase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version(Vec<VersionPart>);

impl Version {
    pub fn new(mut v: &str) -> Self {
        let mut out = vec![];

        while let Some((part, rest)) = VersionPart::chomp(v) {
            out.push(part);
            v = rest;
        }

        Self(out)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut left = self.0.iter();
        let mut right = other.0.iter();
        let mut prev = (left.next(), right.next());

        while let (Some(l), Some(r)) = prev {
            match l.cmp(r) {
                Ordering::Less => return Ordering::Less,
                Ordering::Equal => (),
                Ordering::Greater => return Ordering::Greater,
            }
            prev = (left.next(), right.next());
        }

        #[allow(clippy::match_same_arms)] // merging would be lopsided
        match prev {
            (Some(VersionPart::Dash), None) => Ordering::Less,
            (None, Some(VersionPart::Dash)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (Some(_), Some(_)) => unreachable!(),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for p in &self.0 {
            write!(f, "{p}")?;
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

fn count_digits(s: &str) -> usize {
    s.chars().skip_while(|&c| c == '0').count()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VersionPart {
    Dash,
    Number(String),
    String(String),
}

impl VersionPart {
    fn chomp(inp: &str) -> Option<(Self, &str)> {
        if let Some((&b, r)) = inp.as_bytes().split_first() {
            if b == b'-' {
                return Some((Self::Dash, &inp[1..]));
            }

            if b.is_ascii_digit() {
                let p = r
                    .iter()
                    .position(|&c| !c.is_ascii_digit())
                    .map_or_else(|| inp.len(), |p| p + 1);
                return Some((Self::Number(inp[..p].to_string()), &inp[p..]));
            }
            let p = r
                .iter()
                .position(|&c| c.is_ascii_digit() || c == b'-')
                .map_or_else(|| inp.len(), |p| p + 1);
            return Some((Self::String(inp[..p].to_string()), &inp[p..]));
        }

        None
    }
}

impl Ord for VersionPart {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Number(l), Self::Number(r)) => {
                count_digits(l).cmp(&count_digits(r)).then_with(|| l.cmp(r))
            }
            (Self::String(l), Self::String(r)) => l.cmp(r),
            (Self::Dash, Self::Dash) => Ordering::Equal,
            (Self::Number(_), Self::String(_)) | (Self::Dash, _) => Ordering::Less,
            (Self::String(_), Self::Number(_)) | (_, Self::Dash) => Ordering::Greater,
        }
    }
}

impl PartialOrd for VersionPart {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for VersionPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Dash => "-",
            Self::Number(s) | Self::String(s) => s,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Version;

    #[test]
    fn version_sort() {
        let mut versions = [
            "3.14.16",
            "16.15.1",
            "10.12.15",
            "9.17.1",
            "11.5.8",
            "14.17.8",
            "7.15.1a",
            "4.6.10",
            "5.12.4",
            "7.15.1-pre.2",
            "1.1.10",
            "5.15.16",
            "0.16.12",
            "11.11.8",
            "8.10.8",
            "18.5.5",
            "7.15.2",
            "10.10.13",
            "3.0.0",
            "19.2.17",
            "7.15.1",
            "10.12.14",
            "7.15",
            "16.15.1",
            "7.15.1-pre.1",
        ]
        .iter()
        .map(|&s| Version::new(s))
        .collect::<Vec<_>>();

        versions.sort();

        assert_eq!(
            versions,
            [
                "0.16.12",
                "1.1.10",
                "3.0.0",
                "3.14.16",
                "4.6.10",
                "5.12.4",
                "5.15.16",
                "7.15",
                "7.15.1-pre.1",
                "7.15.1-pre.2",
                "7.15.1",
                "7.15.1a",
                "7.15.2",
                "8.10.8",
                "9.17.1",
                "10.10.13",
                "10.12.14",
                "10.12.15",
                "11.5.8",
                "11.11.8",
                "14.17.8",
                "16.15.1",
                "16.15.1",
                "18.5.5",
                "19.2.17",
            ]
            .iter()
            .map(|&s| Version::new(s))
            .collect::<Vec<_>>()
        );
    }
}
