// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: EUPL-1.2

use base64ct::{Base64, Base64UrlUnpadded, Encoding};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::{
    error::Error,
    path::PathBuf,
    time::{Duration, SystemTime},
};

pub struct TokenHandler {
    hmac: Hmac<Sha256>,
}

impl TokenHandler {
    pub fn new(key: &[u8]) -> Self {
        let hmac = Hmac::new_from_slice(key).expect("hmac should accept keys of any length");
        Self { hmac }
    }

    pub fn generate(&self, duration: Duration) -> [u8; 64] {
        let mut token = [0u8; 48];

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("no time traveling");
        token[0..8].copy_from_slice(&now.as_secs().to_be_bytes());
        token[8..16].copy_from_slice(&(now + duration).as_secs().to_be_bytes());

        let tag = self.hmac.clone().chain_update(&token[0..16]).finalize();
        token[16..48].copy_from_slice(tag.as_bytes());

        let mut out = [0u8; 64];
        Base64UrlUnpadded::encode(&token, &mut out).expect("length should fit");
        out
    }

    pub fn verify(&self, token: impl AsRef<[u8]>) -> bool {
        if let Ok(mut token) = <[u8; 64]>::try_from(token.as_ref())
            && let Ok(decoded) = Base64UrlUnpadded::decode_in_place(&mut token)
            && let Ok(decoded) = <[u8; 48]>::try_from(decoded)
            && self
                .hmac
                .clone()
                .chain_update(&decoded[0..16])
                .verify_slice(&decoded[16..48])
                .is_ok()
            && let Ok(now) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
            && let Ok(beginning) = decoded[0..8].try_into()
            && now.as_secs() >= u64::from_be_bytes(beginning)
            && let Ok(expiration) = decoded[8..16].try_into()
            && now.as_secs() < u64::from_be_bytes(expiration)
        {
            return true;
        }
        false
    }

    pub fn verify_header(&self, value: impl AsRef<[u8]>) -> bool {
        if let Some(split) = value.as_ref().iter().position(|&b| b == b' ')
            && let (kind, rest) = value.as_ref().split_at(split)
        {
            let rest = &rest[1..];
            let mut unbasic;

            let token = if kind.eq_ignore_ascii_case(b"basic") {
                unbasic = [0u8; 70];
                let Ok(unbasic) = Base64::decode(rest, &mut unbasic) else {
                    return false;
                };
                let Some(token) = unbasic.strip_prefix(b"token:") else {
                    return false;
                };
                token
            } else if kind.eq_ignore_ascii_case(b"bearer") {
                rest
            } else {
                return false;
            };

            return self.verify(token);
        }
        false
    }
}

/// generate a token
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "token")]
#[argh(help_triggers("-h", "--help"))]
pub struct Args {
    #[argh(positional)]
    key_file: PathBuf,
    #[argh(positional)]
    minutes: u64,
}

pub async fn generate(args: Args) -> Result<(), Box<dyn Error>> {
    let key = tokio::fs::read(args.key_file).await?;
    let handler = TokenHandler::new(&key);
    let token = handler.generate(Duration::from_mins(args.minutes));
    println!("{}", str::from_utf8(&token).expect("base64 is valid utf-8"));

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::token::TokenHandler;
    use std::time::Duration;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn generate_verify() {
        let handler = TokenHandler::new(b"meowmeow");

        let token = handler.generate(Duration::from_secs(6));
        assert!(handler.verify(token));

        let token = handler.generate(Duration::from_secs(0));
        assert!(!handler.verify(token));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn hardcoded() {
        let handler = TokenHandler::new(b"hunter2");

        // this will break in 10 years lol
        assert!(handler.verify("AAAAAGsExX4AAAAAfdDIfrp7zCcMGDOuZHi1WBMrZrtF24GCs-MT3yTeLWGIYiOn"));
        assert!(handler.verify_header(
            "bearer AAAAAGsExX4AAAAAfdDIfrp7zCcMGDOuZHi1WBMrZrtF24GCs-MT3yTeLWGIYiOn"
        ));
        assert!(handler.verify_header("basic dG9rZW46QUFBQUFHc0V4WDRBQUFBQWZkRElmcnA3ekNjTUdET3VaSGkxV0JNclpydEYyNEdDcy1NVDN5VGVMV0dJWWlPbg=="));

        // expired
        assert!(
            !handler.verify("AAAAAGsExPsAAAAAawTHUyADW4yNrMUJrW_tff3fpC5wOLBQGZeL59imqo3UZDsU")
        );
        assert!(!handler.verify_header("basic dG9rZW46QUFBQUFHc0V4UHNBQUFBQWF3VEhVeUFEVzR5TnJNVUpyV190ZmYzZnBDNXdPTEJRR1plTDU5aW1xbzNVWkRzVQ=="));
    }
}
