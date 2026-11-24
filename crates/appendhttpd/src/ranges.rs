// SPDX-FileCopyrightText: 2024 xfnw
// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: EUPL-1.2

use hyper::header::HeaderValue;
use std::{cmp::min, ops::RangeInclusive, str::FromStr};

/// read the value of an HTTP Range header and parse into a range
///
/// will not return an empty or out of bounds range, should be
/// fine to pass to Bytes.slice after unwrapping
pub fn parse(range: &HeaderValue, len: u64) -> Option<RangeInclusive<u64>> {
    if len == 0 {
        return None;
    }

    let range = range.as_ref();

    if !range.starts_with(b"bytes=") {
        return None;
    }

    let (left, right) = range[6..]
        .split(|&b| b == b',')
        .map(<[u8]>::trim_ascii)
        .map(|range| {
            // TODO: use split_once once slice_split_once stabilized
            // https://github.com/rust-lang/rust/issues/112811
            let position = range.iter().position(|&b| b == b'-')?;
            let left = &range[..position];
            let right = &range[position + 1..];
            // TODO: use from_ascii once int_from_ascii stabilized
            // https://github.com/rust-lang/rust/issues/134821
            let left: Option<u64> = str::from_utf8(left)
                .ok()
                .and_then(|s| u64::from_str(s).ok());
            if !right.iter().all(|&b| b.is_ascii_digit()) {
                return None;
            }
            let right: Option<u64> = str::from_utf8(right)
                .ok()
                .and_then(|s| u64::from_str(s).ok());
            Some((left?, right.unwrap_or(u64::MAX)))
        })
        .reduce(|a, b| {
            let a = a?;
            let b = b?;
            Some((std::cmp::min(a.0, b.0), std::cmp::max(a.1, b.1)))
        })??;

    // http ranges are inclusive
    let len = len - 1;

    let right = min(right, len);
    if left > right {
        return None;
    }

    Some(left..=right)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unreadable_literal)]

    use super::*;

    macro_rules! assert_parse {
        ($inp:expr, $len:expr, $expect:expr) => {
            assert_eq!(parse(&HeaderValue::from_static($inp), $len), $expect);
        };
        ($inp:expr, $len:expr) => {
            assert_parse!($inp, $len, None);
        };
        ($(($inp:expr, $len:expr, $expect:expr)),* $(,)?) => {$(
            assert_parse!($inp, $len, Some($expect));
        )*};
        ($(($inp:expr, $len:expr)),* $(,)?) => {$(
            assert_parse!($inp, $len);
        )*};
    }

    #[test]
    fn valid_ranges() {
        assert_parse!(
            ("bytes=0-0", 1, 0..=0),
            ("bytes=0-", 43, 0..=42),
            ("bytes=5-9", 10, 5..=9),
            ("bytes=5-10", 10, 5..=9),
            ("bytes=5-90", 10, 5..=9),
            ("bytes=5-90, 4-5", 10, 4..=9),
            ("bytes=5-90 ", 10, 5..=9),
            ("bytes=555555-", 1000000, 555555..=999999),
            ("bytes=0-9999999999999999999999999", 621, 0..=620),
            ("bytes=50-60, 69-200, 20-25", 1000, 20..=200),
            ("bytes=1-5, 10-, 20-21", 30, 1..=29),
        );
    }

    #[test]
    fn invalid_ranges() {
        assert_parse!(
            ("bytes=0-0", 0),
            ("bytes=69-", 42),
            ("bytes=69-420", 31),
            ("bytes=420-69", 621),
            ("bytes=3-3.14", 3621),
            ("boots=5-9", 10),
            ("bytes==5-9", 10),
            ("bytes=431000000000000000000000-", 431),
            ("bytes=-10", 20),
            ("bytes=3.14-", 60),
        );
    }
}
