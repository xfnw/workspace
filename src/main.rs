use clap::{Parser, Subcommand};

mod uni;

#[derive(Debug, Parser)]
struct Opt {
    #[command(subcommand)]
    command: Cmds,
}

#[derive(Debug, Subcommand)]
enum Cmds {
    /// decode unicode characters
    Uni(uni::Args),
}

fn main() {
    let opt = Opt::parse();

    match &opt.command {
        Cmds::Uni(args) => uni::run(args),
    }
}
