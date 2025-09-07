//! see [`repr::Operand`] and [`repr::Instruction`] for information about the assembly syntax

use argh::{FromArgs, from_env};
use std::{io::Write, path::PathBuf, process::ExitCode};

mod assemble;
mod h16;
mod parse;
mod repr;

/// vulpine's vm16 assembler
#[derive(Debug, FromArgs)]
#[argh(help_triggers("-h", "--help"))]
struct Opt {
    /// where to send assembled output (dumps hex to stdout by default)
    #[argh(option, short = 'o')]
    output: Option<PathBuf>,
    /// output using vm16's h16 format.
    ///
    /// takes an address for the starting position in hex
    #[argh(option, arg_name = "start", from_str_fn(parse_hex16))]
    h16: Option<u16>,
    #[argh(positional)]
    file: Option<PathBuf>,
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

fn parse_hex16(inp: &str) -> Result<u16, String> {
    u16::from_str_radix(inp, 16).map_err(|e| e.to_string())
}

fn run(opt: &Opt) -> Result<(), Error> {
    let input = if let Some(file) = &opt.file {
        std::fs::read_to_string(file)?
    } else {
        std::io::read_to_string(std::io::stdin())?
    };
    let assembled = assemble::assemble(parse::parse(&input)?)?;

    if let Some(output) = &opt.output {
        let mut file = std::fs::File::create(output)?;
        if let Some(start) = opt.h16 {
            write!(file, "{}", h16::H16Display::new(start, &assembled))?;
        } else {
            let bytes: Vec<_> = assembled.into_iter().flat_map(u16::to_be_bytes).collect();
            file.write_all(&bytes)?;
        }
    } else if let Some(start) = opt.h16 {
        print!("{}", h16::H16Display::new(start, &assembled));
    } else {
        for word in assembled {
            println!("{word:x}");
        }
    }

    Ok(())
}

fn main() -> ExitCode {
    let opt: Opt = from_env();

    if let Err(e) = run(&opt) {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
