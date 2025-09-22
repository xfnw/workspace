// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::unreadable_literal)]

use chrono::{DateTime, offset::Utc};
use std::time::{SystemTime, UNIX_EPOCH};

/// cursed time format
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "now")]
#[argh(help_triggers("-h", "--help", "help"))]
pub struct Args {
    #[argh(positional)]
    seed: u64,
    #[argh(subcommand)]
    action: Action,
}

/// what action to do
#[derive(Clone, Debug, argh::FromArgs)]
#[argh(subcommand)]
enum Action {
    Encode(EncodeAction),
    Convert(ConvertAction),
    Decode(DecodeAction),
}

/// format the current time
#[derive(Clone, Debug, argh::FromArgs)]
#[argh(subcommand, name = "encode")]
#[argh(help_triggers("-h", "--help"))]
struct EncodeAction {
    /// unit of time to round to
    #[argh(positional)]
    accuracy: Accuracy,
}

/// format a specified time
#[derive(Clone, Debug, argh::FromArgs)]
#[argh(subcommand, name = "convert")]
#[argh(help_triggers("-h", "--help"))]
struct ConvertAction {
    #[argh(positional)]
    timestamp: DateTime<Utc>,
}

/// decode the time to a normal format
#[derive(Clone, Debug, argh::FromArgs)]
#[argh(subcommand, name = "decode")]
#[argh(help_triggers("-h", "--help"))]
struct DecodeAction {
    #[argh(positional)]
    blob: String,
}

#[derive(Clone, Copy, Debug)]
enum Accuracy {
    Second = 1,
    Minute = 60,
    Hour = 3600,
    Day = 86400,
    Week = 604800,
}

impl std::str::FromStr for Accuracy {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "second" | "s" => Self::Second,
            "minute" | "m" => Self::Minute,
            "hour" | "h" => Self::Hour,
            "day" | "d" => Self::Day,
            "week" | "w" => Self::Week,
            _ => {
                return Err("accuracy should be second, minute, hour, day, or week");
            }
        })
    }
}

const ALPHABET: &[u8] = b"[\\]^_abcdefghijklmnopqrstuvwxyz|";
const ROUNDS: &[u64] = &[
    11549304151469118491,
    10491820271401555799,
    10212081998298738769,
    15818754901195348703,
    13554773036266474339,
    9559873045211917873,
];
const BITS: u8 = 40;
const MASK: u64 = (1 << BITS) - 1;

fn round_func(inp: u64, seed: u64, round: u64) -> u64 {
    inp.overflowing_mul(round).0 ^ seed
}

fn fe(inp: u128, seed: u64) -> u128 {
    let mut left = (inp >> BITS) as u64;
    let mut right = inp as u64 & MASK;

    for round in ROUNDS {
        left ^= round_func(right, seed, *round) & MASK;
        (left, right) = (right, left);
    }

    (u128::from(left) << BITS) + u128::from(right)
}

fn unfe(inp: u128, seed: u64) -> u128 {
    let mut left = (inp >> BITS) as u64;
    let mut right = inp as u64 & MASK;

    for round in ROUNDS.iter().rev() {
        right ^= round_func(left, seed, *round) & MASK;
        (left, right) = (right, left);
    }

    (u128::from(left) << BITS) + u128::from(right)
}

fn b32(mut inp: u128) -> String {
    let mut out = String::new();
    out.push(ALPHABET[(inp & 31) as usize] as char);
    inp >>= 5;

    while inp > 0 {
        out.push(ALPHABET[(inp & 31) as usize] as char);
        inp >>= 5;
    }

    out
}

fn unb32(inp: &str) -> Option<u128> {
    let mut out: u128 = 0;

    for c in inp.chars().rev() {
        out <<= 5;
        out += ALPHABET.binary_search(&(c as u8)).ok()? as u128;
    }

    Some(out)
}

pub fn run(args: &Args) {
    match &args.action {
        Action::Encode(EncodeAction { accuracy }) => {
            let unix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let unix = unix / *accuracy as u64 * *accuracy as u64;
            let ob = fe(unix.into(), args.seed);
            println!("{}", b32(ob));
        }
        Action::Convert(ConvertAction { timestamp }) => {
            let unix = timestamp.timestamp();
            #[allow(clippy::cast_sign_loss)]
            let ob = fe(u128::from(unix as u64), args.seed);
            println!("{}", b32(ob));
        }
        Action::Decode(DecodeAction { blob }) => {
            let ob = unb32(blob).expect("not in alphabet");
            let unix: u64 = unfe(ob, args.seed).try_into().expect("not a time");
            #[allow(clippy::cast_possible_wrap)]
            let time = DateTime::from_timestamp(unix as i64, 0).unwrap();
            println!("{time}");
        }
    }
}

#[test]
fn refe() {
    for i in 0..100 {
        let f = fe(i, 6);
        assert_eq!(unfe(f, 6), i);
    }
}

#[test]
fn reb32() {
    for i in 0..100 {
        let s = b32(i);
        assert_eq!(unb32(&s), Some(i));
    }
}
