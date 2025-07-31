use chrono::{DateTime, offset::Utc};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, Ipv6Addr},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Actions,
}

#[derive(Debug, clap::Subcommand)]
enum Actions {
    Generate {
        ip: IpAddr,
        difficulty: u8,
        time: Option<u64>,
    },
    Show {
        token: String,
    },
}

fn unixtime() -> u64 {
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("your clock is screwed");
    duration.as_secs()
}

fn gen_challenge(ip: &IpAddr, time: u64) -> Vec<u8> {
    let mut out = time.to_le_bytes().to_vec();
    out.extend(
        match &ip {
            IpAddr::V4(ip) => ip.to_ipv6_mapped(),
            IpAddr::V6(ip) => *ip,
        }
        .to_bits()
        .to_le_bytes(),
    );
    out
}

#[inline]
fn check(n: u64, challenge: [u8; 24], difficulty: u8) -> bool {
    let hash = Sha256::new_with_prefix(n.to_be_bytes())
        .chain_update(challenge)
        .finalize();
    // SAFETY: sha256 always returns 256 bits, so we can skip the bounds check
    let high =
        u128::from_be_bytes(unsafe { hash.get_unchecked(0..16).try_into().unwrap_unchecked() });
    high.leading_zeros() >= difficulty.into()
}

#[test]
fn verify() {
    assert_eq!(Sha256::digest(b"meow").len(), 32);
    assert!(check(80, [0; 24], 6));
    assert!(!check(80, [0; 24], 7));
}

fn generate(ip: &IpAddr, difficulty: u8, time: Option<u64>) {
    if difficulty > 128 {
        eprintln!("i cut corners so difficulty > 128 is not supported");
        return;
    }
    let time = time.unwrap_or_else(unixtime);
    let challenge: [u8; 24] = gen_challenge(ip, time).try_into().unwrap();

    assert_eq!(Sha256::output_size(), 32);
    let result = (0..u64::MAX)
        .into_par_iter()
        .find_any(|n| check(*n, challenge, difficulty))
        .unwrap();

    let mut combined = result.to_be_bytes().to_vec();
    combined.extend(challenge);
    let encoded = base64_light::base64_encode_bytes(&combined);
    println!("{encoded}");
}

fn show(token: &str) {
    let decoded = base64_light::base64_decode(token);
    if decoded.len() != 32 {
        eprintln!("wrong length");
        return;
    }
    let nonce = u64::from_be_bytes(decoded[0..8].try_into().unwrap());
    println!("nonce: {nonce:016x}");
    let time = u64::from_le_bytes(decoded[8..16].try_into().unwrap());
    let ptime: DateTime<Utc> = (UNIX_EPOCH + Duration::from_secs(time)).into();
    let etime: DateTime<Utc> = (UNIX_EPOCH + Duration::from_secs(time + 604_800)).into();
    println!(
        "time: {time} ({}, not before {ptime}, not after {etime})",
        if unixtime().checked_sub(time).unwrap_or(621_926) > 604_800 {
            "expired"
        } else {
            "valid"
        }
    );
    let ip = Ipv6Addr::from_bits(u128::from_le_bytes(decoded[16..32].try_into().unwrap()));
    println!("ip: {ip}");
    let hash = Sha256::digest(&decoded);
    let high = u128::from_be_bytes(hash[0..16].try_into().unwrap());
    let low = u128::from_be_bytes(hash[16..32].try_into().unwrap());
    println!("hash: {high:032x}{low:032x}");
    let mut zeros = high.leading_zeros();
    if zeros == 128 {
        zeros += low.leading_zeros();
    }
    println!("zeros: {zeros}");
}

pub fn run(args: &Args) {
    match &args.action {
        Actions::Generate {
            ip,
            difficulty,
            time,
        } => generate(ip, *difficulty, *time),
        Actions::Show { token } => show(token),
    }
}
