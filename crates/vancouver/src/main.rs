// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use argh::{FromArgs, from_env};
use std::{path::PathBuf, process::ExitCode};

mod check;
mod metadata;
mod types;

/// a more helpful vet
#[derive(Debug, FromArgs)]
struct Opt {
    #[argh(subcommand)]
    command: Cmds,
}

#[derive(Debug, FromArgs)]
#[argh(subcommand)]
enum Cmds {
    Check(CheckArgs),
}

/// do a checkup on your dependencies
#[derive(Debug, FromArgs)]
#[argh(subcommand)]
#[argh(name = "check")]
pub struct CheckArgs {
    /// path to your cargo lock
    #[argh(option, default = "PathBuf::from(\"Cargo.lock\")")]
    lock: PathBuf,
}

fn main() -> ExitCode {
    let opt: Opt = from_env();
    match match opt.command {
        Cmds::Check(args) => check::do_check(&args),
    } {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::from(2)
        }
    }
}
