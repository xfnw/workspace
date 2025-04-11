use ada_url::{HostType, Url};
use std::{cmp::Ord, convert::From, fmt, fs::read_to_string, path::PathBuf};

#[derive(Debug, clap::Args)]
pub struct Args {
    #[arg(default_value = "/dev/stdin")]
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
            .then(self.url.port().cmp(other.url.port()))
            .then(self.url.pathname().cmp(other.url.pathname()))
            .then(self.url.search().cmp(other.url.search()))
            .then(self.url.hash().cmp(other.url.hash()))
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
        let parsed = if let Ok(url) = Url::parse(&inp, None) {
            let host = url.hostname();
            let domain_parts = if url.host_type() == HostType::Domain {
                host.rsplit('.').map(str::to_string).collect()
            } else {
                vec![host.to_string()]
            };
            Ok(ParsedUrl { url, domain_parts })
        } else {
            let domain_parts = inp.rsplit('.').map(str::to_string).collect();
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
    for name in &args.files {
        for line in read_to_string(name).unwrap().lines() {
            urls.push(InfailableUrl::from(line.to_string()));
        }
    }
    urls.sort();
    for url in urls {
        println!("{url}");
    }
}
