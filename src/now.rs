use chrono::{DateTime, offset::Utc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, clap::Args)]
pub struct Args {
    seed: u64,
    #[command(subcommand)]
    action: Action,
}

#[derive(Clone, Debug, clap::Subcommand)]
enum Action {
    Encode {
        #[arg(value_enum)]
        accuracy: Accuracy,
    },
    Decode {
        timestamp: String,
    },
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Accuracy {
    Second = 1,
    Minute = 60,
    Hour = 3600,
    Day = 86400,
    Week = 604800,
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
        Action::Encode { accuracy } => {
            let unix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let unix = unix / *accuracy as u64 * *accuracy as u64;
            let ob = fe(unix.into(), args.seed);
            println!("{}", b32(ob));
        }
        Action::Decode { timestamp } => {
            let ob = unb32(timestamp).expect("not in alphabet");
            let unix = unfe(ob, args.seed).try_into().expect("not a time");
            let time: DateTime<Utc> = (UNIX_EPOCH + Duration::from_secs(unix)).into();
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
