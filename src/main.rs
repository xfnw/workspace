use clap::{Parser, Subcommand};

mod fmt;
mod uni;

#[derive(Debug, Parser)]
struct Opt {
    #[command(subcommand)]
    command: Cmds,
}

#[derive(Debug, Subcommand)]
enum Cmds {
    /// format tcz info-like files
    Fmt(fmt::Args),
    /// decode unicode characters
    Uni(uni::Args),
}

fn main() {
    let opt = Opt::parse();

    match &opt.command {
        Cmds::Fmt(args) => fmt::run(args),
        Cmds::Uni(args) => uni::run(args),
    }
}
