// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

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

#[must_use]
#[allow(clippy::missing_panics_doc, reason = "should be unreachable")]
pub fn tohex_vec<const D: usize>(inp: [u8; D]) -> Vec<u8> {
    let mut out = Vec::with_capacity(D * 2);

    for b in inp {
        out.push(tohex_nibble(b >> 4));
        out.push(tohex_nibble(b & 0b1111));
    }

    assert_eq!(out.len(), D * 2);

    out
}

fn unhex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 0xa),
        _ => None,
    }
}

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

#[test]
fn check_unhex() {
    let expect = [
        0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd,
        0xef,
    ];
    assert_eq!(
        unhex_array(b"1234567890abcdef1234567890abcdef"),
        Some(expect)
    );
}

#[test]
fn hex_round_trip() {
    assert_eq!(
        tohex_vec(unhex_array::<16>(b"33c6c2397a1b079e903c474df792d0e2").unwrap()),
        *b"33c6c2397a1b079e903c474df792d0e2"
    );
}
