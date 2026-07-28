// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

//! encoding and decoding hex with fixed-size arrays

use hybrid_array::{Array, ArraySize, AssocArraySize};
use std::ops::Mul;
use typenum::{Prod, U2};

fn tohex_nibble(n: u8) -> u8 {
    match n {
        0..=9 => n + b'0',
        0xa..=0xf => n + b'a' - 0xa,
        _ => panic!("that is not a nibble"),
    }
}

/// convert from a fixed-size array of bytes to a fixed-size array of hex
#[must_use]
pub fn tohex_array<const D: usize>(
    inp: [u8; D],
) -> Array<u8, Prod<<[u8; D] as AssocArraySize>::Size, U2>>
where
    [u8; D]: AssocArraySize,
    <[u8; D] as AssocArraySize>::Size: Mul<U2>,
    Prod<<[u8; D] as AssocArraySize>::Size, U2>: ArraySize,
{
    let mut out = <Array<u8, Prod<<[u8; D] as AssocArraySize>::Size, U2>>>::default();

    for (i, b) in inp.iter().enumerate() {
        out[i * 2] = tohex_nibble(b >> 4);
        out[i * 2 + 1] = tohex_nibble(b & 0b1111);
    }

    out
}

/// convert from bytes to a vec of hex
#[must_use]
#[allow(clippy::missing_panics_doc, reason = "should be unreachable")]
pub fn tohex_vec(inp: impl AsRef<[u8]>) -> Vec<u8> {
    let inp = inp.as_ref();
    let mut out = Vec::with_capacity(inp.len() * 2);

    for b in inp {
        out.push(tohex_nibble(b >> 4));
        out.push(tohex_nibble(b & 0b1111));
    }

    assert_eq!(out.len(), inp.len() * 2);

    out
}

/// convert from bytes to a hex string
#[must_use]
pub fn tohex_string(inp: impl AsRef<[u8]>) -> String {
    let hex = tohex_vec(inp);

    // SAFETY: 0-9a-fA-F is always valid utf-8
    unsafe { String::from_utf8_unchecked(hex) }
}

fn unhex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 0xa),
        b'A'..=b'F' => Some(b - b'A' + 0xa),
        _ => None,
    }
}

/// convert from hex to a fixed-size array of bytes
#[must_use]
pub fn unhex_array<const D: usize>(inp: &[u8]) -> Option<[u8; D]> {
    if inp.len() != D * 2 {
        return None;
    }

    let (chunks, []) = inp.as_chunks::<2>() else {
        unreachable!();
    };

    let mut out = [0; D];

    for (i, &[h, l]) in chunks.iter().enumerate() {
        out[i] = (unhex_nibble(h)? << 4) | unhex_nibble(l)?;
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn check_unhex() {
        let expect = [
            0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
            0xcd, 0xef,
        ];
        assert_eq!(
            unhex_array(b"1234567890abcdef1234567890ABCDEF"),
            Some(expect)
        );
    }

    #[test]
    fn hex_round_trip() {
        assert_eq!(
            tohex_array(unhex_array::<16>(b"33c6c2397a1b079e903c474df792d0e2").unwrap()),
            *b"33c6c2397a1b079e903c474df792d0e2"
        );
    }

    #[test]
    fn to_string() {
        assert_eq!(tohex_string(*b"meow :3"), "6d656f77203a33");
    }
}
