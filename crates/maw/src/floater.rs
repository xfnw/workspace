// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use half::{bf16, f16};

/// show the error for floats
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "floater")]
#[argh(help_triggers("-h", "--help"))]
pub struct Args {
    #[argh(positional)]
    size: Size,
    #[argh(positional)]
    number: f64,
}

#[derive(Clone, Copy, Debug)]
enum Size {
    F64,
    F32,
    F16,
    BF16,
}

impl std::str::FromStr for Size {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "f64" => Self::F64,
            "f32" => Self::F32,
            "f16" => Self::F16,
            "bf16" => Self::BF16,
            _ => {
                return Err("size should be f64, f32, f16, or bf16");
            }
        })
    }
}

pub fn run(args: &Args) {
    println!(
        "{}",
        match args.size {
            Size::F64 => {
                let float = args.number;
                let new = f64::from_bits(float.to_bits() + 1);
                new - float
            }
            Size::F32 => {
                #[allow(clippy::cast_possible_truncation)]
                let float = args.number as f32;
                let new = f32::from_bits(float.to_bits() + 1);
                (new - float).into()
            }
            Size::F16 => {
                let float = f16::from_f64(args.number);
                let new = f16::from_bits(float.to_bits() + 1);
                (new - float).into()
            }
            Size::BF16 => {
                let float = bf16::from_f64(args.number);
                let new = bf16::from_bits(float.to_bits() + 1);
                (new - float).into()
            }
        }
    );
}
