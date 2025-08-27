use argh::{FromArgs, from_env};
use std::path::PathBuf;

mod assemble;
mod parse;
mod repr;

/// a toy vm16 assembler
#[derive(Debug, FromArgs)]
#[argh(help_triggers("-h", "--help"))]
struct Opt {
    /// where to send assembled output (dumps hex to stdout by default)
    #[argh(option, short = 'o')]
    output: Option<PathBuf>,
    #[argh(positional)]
    file: PathBuf,
}

#[derive(Debug, foxerror::FoxError)]
enum Error {
    /// io error
    #[err(from)]
    Io(std::io::Error),
    /// parse error
    #[err(from)]
    Parse(parse::Error),
    /// assemble error
    #[err(from)]
    Assemble(assemble::Error),
}

fn main() -> Result<(), Error> {
    let opt: Opt = from_env();
    let input = std::fs::read_to_string(&opt.file)?;
    let assembled = assemble::assemble(parse::parse(&input)?)?;

    if let Some(output) = opt.output {
        let bytes: Vec<_> = assembled.into_iter().flat_map(u16::to_be_bytes).collect();
        std::fs::write(output, bytes)?;
    } else {
        for word in assembled {
            println!("{word:x}");
        }
    }

    Ok(())
}
