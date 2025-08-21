#![allow(clippy::unreadable_literal)]

use std::fmt;

/// convert numbers to binary prefixes
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "human")]
pub struct Args {
    #[argh(positional)]
    number: u128,
}

enum Prefix {
    None,
    Ki,
    Mi,
    Gi,
    Ti,
    Pi,
    Ei,
    Zi,
    Yi,
    Ri,
    Qi,
}

impl Prefix {
    fn get(num: u128) -> (u128, Self) {
        macro_rules! leggies {
            ($num:expr, $(($prefix:ident, $min:expr)),*) => {
                match $num {
                    $($min.. => ($num/$min, Prefix::$prefix),)*
                    _ => ($num, Prefix::None),
                }
            }
        }
        leggies!(
            num,
            (Qi, 1267650600228229401496703205376),
            (Ri, 1237940039285380274899124224),
            (Yi, 1208925819614629174706176),
            (Zi, 1180591620717411303424),
            (Ei, 1152921504606846976),
            (Pi, 1125899906842624),
            (Ti, 1099511627776),
            (Gi, 1073741824),
            (Mi, 1048576),
            (Ki, 1024)
        )
    }
}

impl fmt::Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::None => Ok(()),
            Self::Ki => f.write_str("Ki"),
            Self::Mi => f.write_str("Mi"),
            Self::Gi => f.write_str("Gi"),
            Self::Ti => f.write_str("Ti"),
            Self::Pi => f.write_str("Pi"),
            Self::Ei => f.write_str("Ei"),
            Self::Zi => f.write_str("Zi"),
            Self::Yi => f.write_str("Yi"),
            Self::Ri => f.write_str("Ri"),
            Self::Qi => f.write_str("Qi"),
        }
    }
}

pub fn run(args: &Args) {
    let (converted, prefix) = Prefix::get(args.number);
    println!("{converted}{prefix}");
}
