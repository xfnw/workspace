//! see [`repr::Operand`] and [`repr::Instruction`] for information about the assembly syntax

use argh::{FromArgs, from_env};
use std::{
    fs::File,
    io::{BufReader, Read, Write},
    path::PathBuf,
    process::ExitCode,
};

mod assemble;
mod disassemble;
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
    /// disassemble instead of assembling
    #[argh(switch, short = 'd')]
    disassemble: bool,
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
    /// h16 error
    #[err(from)]
    H16(h16::Error),
    /// parse int error
    #[err(from)]
    ParseInt(std::num::ParseIntError),
}

fn parse_hex16(inp: &str) -> Result<u16, String> {
    u16::from_str_radix(inp, 16).map_err(|e| e.to_string())
}

fn run(opt: &Opt) -> Result<(), Error> {
    if opt.disassemble {
        let bytes = if let Some(start) = opt.h16 {
            if let Some(f) = &opt.file {
                let file = BufReader::new(File::open(f)?);
                h16::parse(file.bytes().scan((), |(), i| i.ok()), start)?
            } else {
                let stdin = BufReader::new(std::io::stdin());
                h16::parse(stdin.bytes().scan((), |(), i| i.ok()), start)?
            }
        } else if let Some(f) = &opt.file {
            let mut bytes = BufReader::new(File::open(f)?).bytes();
            let mut words = vec![];
            while let (Some(Ok(left)), Some(Ok(right))) = (bytes.next(), bytes.next()) {
                words.push((u16::from(left) << 8) + u16::from(right));
            }
            words
        } else {
            std::io::stdin()
                .lines()
                .flat_map(|s| {
                    s.as_deref()
                        .into_iter()
                        .flat_map(str::split_ascii_whitespace)
                        .map(|s| u16::from_str_radix(s, 16))
                        .collect::<Vec<_>>()
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let disassembled = disassemble::disassemble(&bytes);
        if let Some(output) = &opt.output {
            let mut file = File::create(output)?;
            write!(file, "{disassembled}")?;
        } else {
            print!("{disassembled}");
        }
        return Ok(());
    }

    let input = if let Some(file) = &opt.file {
        std::fs::read_to_string(file)?
    } else {
        std::io::read_to_string(std::io::stdin())?
    };
    let assembled = assemble::assemble(parse::parse(&input)?)?;

    if let Some(output) = &opt.output {
        let mut file = File::create(output)?;
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
