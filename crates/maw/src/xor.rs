// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use std::{
    fs::File,
    io::{BufReader, Read, Write},
    path::PathBuf,
};

/// xor files together
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "xor")]
#[argh(help_triggers("-h", "--help"))]
pub struct Args {
    #[argh(positional)]
    files: Vec<PathBuf>,
}

struct XorIter<R> {
    bufs: Vec<R>,
}

impl<R: Read> Iterator for XorIter<R> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        self.bufs
            .iter_mut()
            .map(|r| {
                // TODO: use Read::read_le once stabilized
                let mut out = [0u8; 1];
                r.read_exact(&mut out).ok()?;
                Some(out[0])
            })
            .reduce(|a, b| Some(a? ^ b?))?
    }
}

pub fn run(args: &Args) {
    let bufs: Vec<_> = args
        .files
        .iter()
        .map(|p| BufReader::new(File::open(p).unwrap()))
        .collect();
    let mut stdout = std::io::stdout().lock();
    let xor = XorIter { bufs };

    for b in xor {
        stdout.write_all(&[b]).unwrap();
    }

    stdout.flush().unwrap();
}
