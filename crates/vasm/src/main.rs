use argh::{FromArgs, from_env};
use std::path::PathBuf;

mod parse;
mod repr;

/// a toy vm16 assembler
#[derive(Debug, FromArgs)]
#[argh(help_triggers("-h", "--help"))]
struct Opt {
    #[argh(positional)]
    file: PathBuf,
}

#[derive(Debug, foxerror::FoxError)]
enum Error {
    /// io error
    #[err(from)]
    IoError(std::io::Error),
    /// parse error
    #[err(from)]
    ParseError(parse::Error),
}

fn main() -> Result<(), Error> {
    let opt: Opt = from_env();
    let input = std::fs::read_to_string(&opt.file)?;

    dbg!(parse::parse(&input)?);

    Ok(())
}
