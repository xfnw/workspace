// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: EUPL-1.2

use std::error::Error;

use argh::{FromArgs, from_env};

mod body;
mod ranges;
mod serve;
mod token;

/// expose append-kv with http
#[derive(Debug, FromArgs)]
#[argh(help_triggers("-h", "--help", "help"))]
struct Opt {
    #[argh(subcommand)]
    command: Cmds,
}

#[derive(Debug, FromArgs)]
#[argh(subcommand)]
enum Cmds {
    Serve(serve::Args),
    Token(token::Args),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opt: Opt = from_env();

    match opt.command {
        Cmds::Serve(args) => serve::serve(args).await,
        Cmds::Token(args) => token::generate(args).await,
    }?;

    Ok(())
}
