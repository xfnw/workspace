// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use argh::{FromArgs, from_env};
use std::{path::PathBuf, process::ExitCode};

mod audit;
mod check;
mod de;
mod merge;
mod metadata;
mod types;

/// dependency auditing that meows
#[derive(Debug, FromArgs)]
#[argh(help_triggers("-h", "--help", "help"))]
struct Opt {
    #[argh(subcommand)]
    command: Cmds,
}

#[derive(Debug, FromArgs)]
#[argh(subcommand)]
enum Cmds {
    Check(CheckArgs),
    Audit(AuditArgs),
    Merge(MergeArgs),
}

/// do a checkup on your dependencies
#[derive(Debug, FromArgs)]
#[argh(subcommand)]
#[argh(name = "check")]
#[argh(help_triggers("-h", "--help"))]
pub struct CheckArgs {
    /// path to your cargo lock
    #[argh(option, default = "PathBuf::from(\"Cargo.lock\")")]
    lock: PathBuf,
    /// path to your vancouver config
    #[argh(option, default = "PathBuf::from(\"vancouver.toml\")")]
    config: PathBuf,
    /// path to your audits file
    #[argh(option, default = "PathBuf::from(\"audits.toml\")")]
    audits: PathBuf,
    /// stop searching after this many layers of recursion
    #[argh(option, default = "621")]
    recursion_limit: usize,
    /// do a check as if there were no exempts in the config
    #[argh(switch)]
    ignore_exempts: bool,
    /// add exempts for all unaudited dependencies to the config
    #[argh(switch)]
    add_exempts: bool,
    /// remove unused exempts from the config
    #[argh(switch)]
    ratchet: bool,
    /// the output format to use (human or json)
    #[argh(option, default = "OutputFormat::Human")]
    output: OutputFormat,
}

#[derive(Debug, PartialEq, Eq)]
enum OutputFormat {
    Human,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            _ => Err("output format must be human or json"),
        }
    }
}

/// record that you audited a dependency
#[derive(Debug, FromArgs)]
#[argh(subcommand)]
#[argh(name = "audit")]
#[argh(help_triggers("-h", "--help"))]
pub struct AuditArgs {
    /// path to your audits file
    #[argh(option, default = "PathBuf::from(\"audits.toml\")")]
    audits: PathBuf,
    /// name of the dependency you audited
    #[argh(positional)]
    name: String,
    /// the previous version you diffed against (delta audit)
    #[argh(option, short = 'b')]
    base: Option<String>,
    /// the current version you audited
    #[argh(positional)]
    version: String,
    /// the criteria you audited
    #[argh(positional)]
    criteria: String,
    /// additional notes to include
    #[argh(option, short = 'n')]
    notes: Option<String>,
    /// record a violation
    #[argh(switch, short = 'x')]
    fail: bool,
}

/// merge audits from another file
#[derive(Debug, FromArgs)]
#[argh(subcommand)]
#[argh(name = "merge")]
#[argh(help_triggers("-h", "--help"))]
pub struct MergeArgs {
    /// path to your audits file
    #[argh(option, default = "PathBuf::from(\"audits.toml\")")]
    audits: PathBuf,
    /// the name you want to use for the merge source
    #[argh(positional)]
    identifier: String,
    /// the path to what you want to merge from
    #[argh(positional, default = "PathBuf::from(\"/dev/stdin\")")]
    file: PathBuf,
    /// set all merged entries private and prefix criteria with
    /// identifier
    #[argh(switch)]
    isolate: bool,
}

fn main() -> ExitCode {
    let opt: Opt = from_env();
    match match opt.command {
        Cmds::Check(args) => check::do_check(&args),
        Cmds::Audit(args) => audit::add_audit(&args),
        Cmds::Merge(args) => merge::do_merge(&args),
    } {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::from(2)
        }
    }
}
