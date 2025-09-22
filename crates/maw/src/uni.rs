// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use unicode_names2::{character, name};

/// decode unicode characters
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "uni")]
#[argh(help_triggers("-h", "--help"))]
pub struct Args {
    /// reverse mode
    #[argh(switch, short = 'r')]
    reverse: bool,
    #[argh(positional, greedy)]
    input: Vec<String>,
}

fn fmt_output(c: char) -> String {
    format!(
        "U+{:04X} {} ({})",
        c as u32,
        name(c).map_or_else(|| "UNDEFINED".to_string(), |n| n.to_string()),
        c
    )
}

pub fn run(args: &Args) {
    if args.reverse {
        for arg in &args.input {
            print!("{}", character(arg).unwrap_or('\u{fffd}'));
        }
        println!();
        return;
    }

    for arg in &args.input {
        let chars = arg.chars();
        let out: Vec<_> = chars.map(fmt_output).collect();
        println!("{}", out.join(" "));
    }
}
