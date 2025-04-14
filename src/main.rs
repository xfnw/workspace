use clap::{Parser, Subcommand};

mod floater;
mod fmt;
mod human;
mod now;
mod sort;
mod uni;

#[derive(Debug, Parser)]
struct Opt {
    #[command(subcommand)]
    command: Cmds,
}

#[derive(Debug, Subcommand)]
enum Cmds {
    /// show the error for floats
    Floater(floater::Args),
    /// format tcz info-like files
    Fmt(fmt::Args),
    /// convert numbers to binary prefixes
    Human(human::Args),
    /// weird time format
    Now(now::Args),
    /// sort urls in domain order
    Sort(sort::Args),
    /// decode unicode characters
    Uni(uni::Args),
}

fn main() {
    let opt = Opt::parse();

    match &opt.command {
        Cmds::Floater(args) => floater::run(args),
        Cmds::Fmt(args) => fmt::run(args),
        Cmds::Human(args) => human::run(args),
        Cmds::Now(args) => now::run(args),
        Cmds::Sort(args) => sort::run(args),
        Cmds::Uni(args) => uni::run(args),
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn verify_clap() {
        use clap::CommandFactory;
        Opt::command().debug_assert();
    }
}
