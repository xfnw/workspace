use chrono::{DateTime, offset::Utc};
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
        #[arg(default_value = "0")]
        time: u64,
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

fn generate(ip: &IpAddr, difficulty: u8, time: u64) {}

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
        } => generate(&ip, *difficulty, *time),
        Actions::Show { token } => show(&token),
    }
}
