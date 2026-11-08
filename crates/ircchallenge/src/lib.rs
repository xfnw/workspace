// SPDX-FileCopyrightText: 2020 jesopo
// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

//! port of [jess' ircchallenge python library], for doing
//! ratbox-style challenges
//!
//! [jess' ircchallenge python library]: https://github.com/jesopo/ircchallenge

use base64ct::{Base64, Encoding};
use rsa::{Oaep, PrivateKey};
use sha1::{Digest, Sha1};
use std::fmt;

pub use rsa;

/// do a ratbox-style challenge
pub struct Challenge {
    key: PrivateKey,
    buf: Vec<u8>,
}

impl Challenge {
    /// create a new [`Challenge`]
    #[must_use]
    pub fn new(key: PrivateKey) -> Self {
        Self { key, buf: vec![] }
    }

    /// add chunk of base64 challenge contents
    ///
    /// this should only be given valid base64. do not include the `:` or
    /// `\r\n` from the irc line, nor any other whitespace.
    pub fn push<T: AsRef<[u8]>>(&mut self, base64: T) {
        self.buf.extend_from_slice(base64.as_ref());
    }

    /// calculate the challenge response
    ///
    /// # Errors
    /// an error will be returned if [`Challenge::push`] was fed data
    /// that is not valid base64 or cannot be decrypted, or this
    /// method got called multiple times without using
    /// [`Challenge::reset`]
    pub fn finalise(&mut self) -> Result<String, Error> {
        let ciphertext = Base64::decode_in_place(&mut self.buf).map_err(Error::Base64)?;

        let padding = Oaep::<Sha1>::new();
        let plaintext = self.key.decrypt(padding, ciphertext).map_err(Error::Rsa)?;

        let hashed = Sha1::digest(&plaintext);

        Ok(Base64::encode_string(&hashed))
    }

    /// reset state in order to do another challenge
    pub fn reset(&mut self) {
        self.buf.clear();
    }
}

#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    Base64(base64ct::InvalidEncodingError),
    Rsa(rsa::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base64(e) => write!(f, "could not decode base64: {e}"),
            Self::Rsa(e) => write!(f, "could not decrypt: {e}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::Challenge;
    use rsa::{PrivateKey, pkcs1::DecodeRsaPrivateKey};

    #[test]
    fn chunks() {
        let key = PrivateKey::read_pkcs1_pem_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/data/chunks.key"
        ))
        .unwrap();
        let mut challenge = Challenge::new(key);
        let ciphertexts = [
            "f9FI0b4qdzv6vtVIFTLe5f+eiQAJ4mOb0Rygef7Dn862RZYnzu6ZJPhaiCSHFLdIEg7bBMX1",
            "2E4S4wrgCXnMgth/9IjaQ0YX1UaeVarDEE/N7lDvdSTcMjhXB/dGEyDLE/g8K4RiQ35ULhdP",
            "f41wpunc0Rsfk6QH9aqKfEpeJ747p+RdNYpiKSHQFwPQDb6jhIqeeCGj2/uI/dYKfUsANAVt",
            "Oud9x6OXnJcPkt1TJT9jLd1d9As3kJxo76I8vh8w+E4JhDdS3wlmpBfn7+vG1+qdk2RveqIv",
            "hEnKDAelfBBWQdS4kKAzbZLjkeXKE2OjNuf8oC4m0Sk1Jzge4+Vttw==",
        ];

        for line in ciphertexts {
            challenge.push(line);
        }

        assert_eq!(
            challenge.finalise().unwrap(),
            "yFMg2d25DBP0ohXx8Kh7UxqzMxA="
        );
    }
}
