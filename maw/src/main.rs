use argh::{FromArgs, from_env};

mod floater;
mod fmt;
mod human;
mod now;
mod sort;
mod uni;
mod uwu;
mod yap;

/// some random utilities
#[derive(Debug, FromArgs)]
struct Opt {
    #[argh(subcommand)]
    command: Cmds,
}

#[derive(Debug, FromArgs)]
#[argh(subcommand)]
enum Cmds {
    Floater(floater::Args),
    Fmt(fmt::Args),
    Human(human::Args),
    Now(now::Args),
    Sort(sort::Args),
    Uni(uni::Args),
    Uwu(uwu::Args),
    Yap(yap::Args),
}

fn main() {
    let opt: Opt = from_env();

    match &opt.command {
        Cmds::Floater(args) => floater::run(args),
        Cmds::Fmt(args) => fmt::run(args),
        Cmds::Human(args) => human::run(args),
        Cmds::Now(args) => now::run(args),
        Cmds::Sort(args) => sort::run(args),
        Cmds::Uni(args) => uni::run(args),
        Cmds::Uwu(args) => uwu::run(args),
        Cmds::Yap(args) => yap::run(args),
    }
}
