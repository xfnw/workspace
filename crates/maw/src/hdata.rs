// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// hide data in html case
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "hdata")]
#[argh(help_triggers("-h", "--help"))]
pub struct Args {
    /// count the number of letters we can hide bits in
    #[argh(switch)]
    count: bool,
    #[argh(positional)]
    html: PathBuf,
    #[argh(positional)]
    data: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
enum Token<'a> {
    /// anything that we should not touch
    Text(&'a [u8]),
    /// weird characters and whitespace etc
    Syntax(&'a u8),
    /// things we can mess with the case of
    Identifier(&'a [u8]),
}

fn text_until(inp: &[u8], predicate: impl Fn(&u8) -> bool) -> Option<(Token<'_>, &[u8])> {
    let pos = inp.iter().position(predicate).unwrap_or(inp.len());
    (pos > 0).then_some((Token::Text(&inp[..pos]), &inp[pos..]))
}

fn syntax(inp: &[u8], predicate: impl Fn(&u8) -> bool) -> Option<(Token<'_>, &[u8])> {
    let (b, rest) = inp.split_first()?;
    if !predicate(b) {
        return None;
    }
    Some((Token::Syntax(b), rest))
}

fn identifier(inp: &[u8]) -> Option<(Token<'_>, &[u8])> {
    let pos = inp
        .iter()
        .position(|&b| b.is_ascii_whitespace() || matches!(b, b'=' | b'>' | b'/'))?;
    (pos > 0).then_some((Token::Identifier(&inp[..pos]), &inp[pos..]))
}

fn quoted(inp: &[u8]) -> Option<([Token<'_>; 3], &[u8])> {
    let (opening, rest) = inp.split_first()?;
    if !matches!(*opening, b'"' | b'\'') {
        return None;
    }
    let (text, rest) = text_until(rest, |b| b == opening)?;
    let (closing, rest) = syntax(rest, |b| b == opening)?;
    Some(([Token::Syntax(opening), text, closing], rest))
}

fn tag(inp: &[u8]) -> Option<(Vec<Token<'_>>, &[u8])> {
    let (opening, rest) = syntax(inp, |&b| b == b'<')?;

    if rest
        .first()
        .is_none_or(|&b| b.is_ascii_whitespace() || b == b'>')
    {
        return None;
    }

    let mut out = Vec::with_capacity(2);
    let mut tail = rest;
    out.push(opening);

    loop {
        if let Some((tok, rest)) = identifier(tail) {
            out.push(tok);
            tail = rest;

            while let Some((tok, rest)) = syntax(tail, |&b| b.is_ascii_whitespace() || b == b'/') {
                out.push(tok);
                tail = rest;
            }

            if let Some((tok, rest)) = syntax(tail, |&b| b == b'=') {
                out.push(tok);
                tail = rest;

                while let Some((tok, rest)) = syntax(tail, |&b| b.is_ascii_whitespace()) {
                    out.push(tok);
                    tail = rest;
                }

                if let Some((toks, rest)) = quoted(tail) {
                    out.extend_from_slice(&toks);
                    tail = rest;
                } else if let Some((tok, rest)) =
                    text_until(tail, |&b| b.is_ascii_whitespace() || b == b'>')
                {
                    out.push(tok);
                    tail = rest;
                }
            }

            continue;
        }

        if let Some((tok, rest)) = syntax(tail, |&b| b == b'>') {
            out.push(tok);
            return Some((out, rest));
        }

        if let Some((tok, rest)) = syntax(tail, |&b| b.is_ascii_whitespace() || b == b'/') {
            out.push(tok);
            tail = rest;
            continue;
        }

        return None;
    }
}

fn tokenize(inp: &[u8]) -> Vec<Token<'_>> {
    let mut out = vec![];
    let mut tail = inp;

    while !tail.is_empty() {
        if let Some((toks, rest)) = tag(tail) {
            out.extend_from_slice(&toks);
            tail = rest;
            continue;
        }
        if let Some((tok, rest)) = text_until(tail, |&b| b == b'<') {
            out.push(tok);
            tail = rest;
            continue;
        }
        if let Some((tok, rest)) = syntax(tail, |&b| b == b'<') {
            out.push(tok);
            tail = rest;
            continue;
        }
        unreachable!();
    }

    out
}

fn read_to_vec(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let mut out = vec![];
    file.read_to_end(&mut out)?;
    Ok(out)
}

struct BitIter<'a> {
    head: u8,
    tail: &'a [u8],
    curbit: u8,
}

impl Iterator for BitIter<'_> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.curbit == 0 {
            let (&new, rest) = self.tail.split_first()?;
            self.head = new;
            self.tail = rest;
            self.curbit = 8;
        }
        self.curbit -= 1;
        Some(self.head & (1 << self.curbit) != 0)
    }
}

#[test]
fn check_bits() {
    let bititer = BitIter {
        head: 0,
        tail: &[0xac, 0xab],
        curbit: 0,
    };
    assert_eq!(
        &bititer.collect::<Vec<_>>(),
        &[
            true, false, true, false, true, true, false, false, true, false, true, false, true,
            false, true, true
        ]
    );
}

pub fn run(args: &Args) {
    let html = read_to_vec(&args.html).unwrap();
    let tokens = tokenize(&html);

    if args.count {
        let bits: usize = tokens
            .iter()
            .filter_map(|t| {
                if let Token::Identifier(i) = t {
                    Some(i.iter().filter(|b| b.is_ascii_alphabetic()).count())
                } else {
                    None
                }
            })
            .sum();
        println!(
            "you can fit {} bytes ({bits} bits) into this html",
            bits / 8
        );
        return;
    }

    let mut stdout = std::io::stdout().lock();

    if let Some(data) = &args.data {
        let data = read_to_vec(data).unwrap();
        let mut dataiter = BitIter {
            head: 0,
            tail: &data,
            curbit: 0,
        };

        for token in &tokens {
            match token {
                Token::Text(t) => stdout.write_all(t).unwrap(),
                Token::Syntax(b) => stdout.write_all(std::slice::from_ref(b)).unwrap(),
                Token::Identifier(i) => {
                    for &b in *i {
                        let c = if !b.is_ascii_alphabetic() {
                            b
                        } else if dataiter.next().unwrap_or(false) {
                            b.to_ascii_uppercase()
                        } else {
                            b.to_ascii_lowercase()
                        };
                        stdout.write_all(std::slice::from_ref(&c)).unwrap();
                    }
                }
            }
        }

        return;
    }

    let mut bitbuf = Vec::with_capacity(8);

    for token in &tokens {
        if let Token::Identifier(i) = token {
            for b in *i {
                if b.is_ascii_alphabetic() {
                    bitbuf.push(b.is_ascii_uppercase());
                    if bitbuf.len() == 8 {
                        let mut o: u8 = 0;
                        for &b in &bitbuf {
                            o <<= 1;
                            o |= u8::from(b);
                        }
                        stdout.write_all(std::slice::from_ref(&o)).unwrap();
                        bitbuf.clear();
                    }
                }
            }
        }
    }

    if !bitbuf.is_empty() {
        let mut o: u8 = 0;
        for &b in bitbuf.iter().chain(std::iter::repeat(&false)).take(8) {
            o <<= 1;
            o |= u8::from(b);
        }
        stdout.write_all(std::slice::from_ref(&o)).unwrap();
    }
}
