// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use std::{cmp::Ord, convert::From, fmt, fs::read_to_string, path::PathBuf};
use url::Url;

/// sort urls in domain order
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "sort")]
#[argh(help_triggers("-h", "--help"))]
pub struct Args {
    #[argh(positional, greedy)]
    files: Vec<PathBuf>,
}

#[derive(Debug, Eq, PartialEq)]
struct ParsedUrl {
    url: Url,
    domain_parts: Vec<String>,
}

impl Ord for ParsedUrl {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.domain_parts
            .cmp(&other.domain_parts)
            .then(self.url.port().cmp(&other.url.port()))
            .then(self.url.path().cmp(other.url.path()))
            .then(self.url.query().cmp(&other.url.query()))
            .then(self.url.fragment().cmp(&other.url.fragment()))
            .then(self.url.cmp(&other.url))
    }
}

impl PartialOrd for ParsedUrl {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Eq, PartialEq)]
struct BareDomain {
    raw: String,
    domain_parts: Vec<String>,
}

impl Ord for BareDomain {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.domain_parts.cmp(&other.domain_parts)
    }
}

impl PartialOrd for BareDomain {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Eq, PartialEq)]
struct InfailableUrl(Result<ParsedUrl, BareDomain>);

impl From<String> for InfailableUrl {
    fn from(inp: String) -> Self {
        let parsed = if let Ok(url) = Url::parse(&inp) {
            let domain_parts = if let Some(host) = url.domain() {
                host.rsplit('.').map(str::to_ascii_lowercase).collect()
            } else if let Some(host) = url.host_str() {
                vec![host.to_ascii_lowercase()]
            } else {
                vec![]
            };
            Ok(ParsedUrl { url, domain_parts })
        } else {
            let domain_parts = inp.rsplit('.').map(str::to_ascii_lowercase).collect();
            Err(BareDomain {
                raw: inp,
                domain_parts,
            })
        };
        Self(parsed)
    }
}

impl Ord for InfailableUrl {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for InfailableUrl {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for InfailableUrl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.0 {
            Ok(p) => p.url.fmt(f),
            Err(s) => s.raw.fmt(f),
        }
    }
}

pub fn run(args: &Args) {
    let mut urls = Vec::new();
    let files = if args.files.is_empty() {
        &vec![PathBuf::from("/dev/stdin")]
    } else {
        &args.files
    };
    for name in files {
        for line in read_to_string(name).unwrap().lines() {
            urls.push(InfailableUrl::from(line.to_string()));
        }
    }
    urls.sort();
    for url in urls {
        println!("{url}");
    }
}
