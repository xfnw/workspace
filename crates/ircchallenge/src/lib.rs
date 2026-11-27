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
    use rsa::{BoxedUint, PrivateKey};

    #[test]
    #[cfg_attr(miri, ignore)] // really slow in miri, should work though
    fn chunks() {
        let key = PrivateKey::from_components(
            BoxedUint::from_be_hex("9c0777babbdf1702ae75f93b80387dab74c6a92920cb4bf756b6afa95bdd43729be9be2ba5d5f1747f22b48bb86784a0ee39d40c4bab0009abec239b91c7b21ae89dfe71e9d56b61567d42aa89421d8ad5c4a524c3ec7282dacfacd6cf9e788f1bd05191203286b9373f0f389987f323eba61a0c8fa13f292ef975997568ef51ed374c2ef85170d9a8b89caae2e648f0b1a65690b0ec3c0ee3c0ca569ad425bea4d733e31cba6142e0f980a4900dec95145272da53a2377bb11477f686e7b16c8963c1dd78ad544fe2bf5425068f44e189baed8f868bd48250a0b2a7373b0fd0a956c75b89d0607907332e386b8a09517d3734877299d330e25302f95e77660b", 2048).unwrap(),
            BoxedUint::from_be_hex("0000000000010001", 64).unwrap(),
            BoxedUint::from_be_hex("286bbed6bf68f1fb1ddb2e2ffdeda490d075cec8fad7efdc29b8b61537d94069e67e4d67dd546972f6a4b50525469c135ce3da4d3d2515a36bc88708fc613180cb16731e6ba78063fd1602394fc9b3018e939cfcc34cf3f5765e2ab6939ec838bac9e2f7a7d1af193878bba6bc0f7e2ba616a7d4ed38a6b6329729a211ca9784c851f451ce58f93afd4fdb33cecef2730cc087662bf3c47a61e506125b6e544cb05d8e1c0e50b86c001c156134abb04106ad8e739069b919400a487c91b49457332885a8e83b3fafadd8f3e22baa2c93765fe2e15b076e8130ccb70f2bfdf55b2844a5e1c53a5966289841a5720e8650d50618f938eb95e936a50e064d73dca1", 2048).unwrap(),
            vec![
                BoxedUint::from_be_hex("ca689cc2b6861e5bf5be2b0d81d27bffd017105a9b74300fc317e9ed05b22469de042c899e6309bd29506fe77913e697c47e96c494cdcc3255cda92e175f1a568e0533080bbc1cb918fe2d33ebfd3115559d37655b9d7f2a576a9f45c237856c16b99113b2e1fbac237c3d8f90d73a3d6aef50dba0b095baa5e7d7c4230923a9", 1024).unwrap(),
                BoxedUint::from_be_hex("c557399ab7e7fa3915a35fad3617bb6b829a47779fc4b2d9e89b7cd074579900fdcbd8214e239cb0dd156556bcbc01e09e6836a8a02419b2b4f07dcca79b8edb3e885006a4943717af4c2dae12837a55f99f372fb7c7ebb9c07cea75b4de5e03fc8eb454470d4b7dc22c93df3b33454d6109394bd53755557a3c5b0415ac0c93", 1024).unwrap(),
            ],
        ).unwrap();
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
