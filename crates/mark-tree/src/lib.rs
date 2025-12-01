// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MIT

//! questionable tree-shaped thing
#![allow(clippy::precedence)]

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// a more flexible way to turn a slice of bools into something
pub trait ConvertBits {
    type Output;
    /// convert a slice of bools into the output type
    fn convert_bits(value: &[bool]) -> Self::Output;
}

impl<T> ConvertBits for T
where
    T: for<'a> From<&'a [bool]>,
{
    type Output = T;
    fn convert_bits(value: &[bool]) -> Self::Output {
        T::from(value)
    }
}

/// an [`Iterator`] over a number of the most significant bits in an unsigned integer
///
/// this goes most significant to least significant. if the range is larger than the number of
/// bits, it will act like the remaining items are unset bits
#[must_use = "iterators do not do anything until consumed"]
#[derive(Debug, Clone)]
pub struct BitRangeIter<T> {
    inner: T,
    range: usize,
}

macro_rules! bitrange_impl {
    ($($type:ident),*) => {$(
        impl From<($type, usize)> for BitRangeIter<$type> {
           fn from(item: ($type, usize)) -> Self {
                Self {
                    inner: item.0,
                    range: item.1,
                }
            }
        }
        impl Iterator for BitRangeIter<$type> {
            type Item = bool;
            fn next(&mut self) -> Option<Self::Item> {
                if self.range == 0 {
                    return None;
                }
                self.range -= 1;

                let is_set = self.inner & 1 << $type::BITS - 1;
                self.inner <<= 1;
                Some(is_set != 0)
            }
        }
    )*}
}

bitrange_impl!(u8, u16, u32, u64, u128);

/// an ip address range
///
/// note that this internally stores ipv4 addresses as ipv4-mapped ipv6 addresses,
/// which has the side effect of coercing ipv6 addresses in the range `::ffff:0:0/96` to ipv4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpRange {
    ip: Ipv6Addr,
    mask_len: usize,
}

impl IpRange {
    /// create a new ip range from either kind of ip address
    #[must_use]
    pub const fn new(ip: IpAddr, mask_len: usize) -> Option<Self> {
        match ip {
            IpAddr::V6(ip) => Self::new_v6(ip, mask_len),
            IpAddr::V4(ip) => Self::new_v4(ip, mask_len),
        }
    }

    /// create a new ip range from an ipv6 address
    #[must_use]
    pub const fn new_v6(ip: Ipv6Addr, mask_len: usize) -> Option<Self> {
        if mask_len > 128 {
            return None;
        }
        Some(Self { ip, mask_len })
    }

    /// create a new ip range from an ipv4 address
    #[must_use]
    pub const fn new_v4(ip: Ipv4Addr, mask_len: usize) -> Option<Self> {
        // `?` is not const :(
        let Some(mask_len) = mask_len.checked_add(96) else {
            return None;
        };
        Self::new_v6(ip.to_ipv6_mapped(), mask_len)
    }

    /// construct an ip range from a vec of bools
    #[must_use]
    pub fn from_bits(bits: &[bool]) -> Option<Self> {
        let mut out = 0;

        for &bit in bits.iter().rev() {
            out >>= 1;
            if bit {
                out |= 1 << (u128::BITS - 1);
            }
        }

        Self::new_v6(Ipv6Addr::from_bits(out), bits.len())
    }

    /// decompose an ip range into an ip address and subnet mask length
    #[must_use]
    pub const fn into_parts(&self) -> (IpAddr, usize) {
        if let Some(v4) = self.ip.to_ipv4_mapped() {
            (IpAddr::V4(v4), self.mask_len - 96)
        } else {
            (IpAddr::V6(self.ip), self.mask_len)
        }
    }

    /// create an iterator over the bits in the ip range
    pub fn iter(&self) -> BitRangeIter<u128> {
        (self.ip.to_bits(), self.mask_len).into()
    }
}

impl ConvertBits for IpRange {
    type Output = Option<Self>;
    fn convert_bits(value: &[bool]) -> Self::Output {
        Self::from_bits(value)
    }
}

impl IntoIterator for &IpRange {
    type Item = bool;
    type IntoIter = BitRangeIter<u128>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::fmt::Display for IpRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (ip, mask_len) = self.into_parts();
        write!(f, "{ip}/{mask_len}")
    }
}

impl std::str::FromStr for IpRange {
    type Err = ParseIpRangeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ip, len) = if let Some((ip, len)) = s.rsplit_once('/') {
            (ip, Some(len))
        } else {
            (s, None)
        };
        let ip = ip.parse().map_err(ParseIpRangeError::AddrParse)?;
        let len = if let Some(len) = len {
            len.parse().map_err(ParseIpRangeError::ParseInt)?
        } else {
            match ip {
                IpAddr::V6(_) => 128,
                IpAddr::V4(_) => 32,
            }
        };
        Self::new(ip, len).ok_or(ParseIpRangeError::MaskTooBig)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseIpRangeError {
    AddrParse(std::net::AddrParseError),
    ParseInt(std::num::ParseIntError),
    MaskTooBig,
}

impl std::fmt::Display for ParseIpRangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AddrParse(a) => a.fmt(f),
            Self::ParseInt(p) => p.fmt(f),
            Self::MaskTooBig => write!(f, "Subnet mask too long"),
        }
    }
}

impl std::error::Error for ParseIpRangeError {}

/// a trie where branches get marked based on where an iterator of bools ends
///
/// probably only useful when [`BitRangeIter`] is used as the iterator
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum MarkTree {
    #[default]
    AllUnmarked,
    AllMarked,
    Branch(Box<MarkTree>, Box<MarkTree>),
}

impl MarkTree {
    /// create a new tree
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// mark the position in the tree dictated by an iterator
    pub fn mark(&mut self, mut bits: impl Iterator<Item = bool>) {
        let new = if let Some(bit) = bits.next() {
            match self {
                Self::AllMarked => return,
                Self::AllUnmarked => {
                    let mut deeper = Self::new();
                    deeper.mark(bits);
                    let mut other = Self::new();

                    if bit {
                        std::mem::swap(&mut deeper, &mut other);
                    }
                    Self::Branch(Box::new(deeper), Box::new(other))
                }
                Self::Branch(a, b) => {
                    let deeper = if bit { b } else { a };
                    deeper.mark(bits);
                    return;
                }
            }
        } else {
            Self::AllMarked
        };
        _ = std::mem::replace(self, new);
    }

    /// unmark the position in the tree dictated by an iterator
    pub fn unmark(&mut self, mut bits: impl Iterator<Item = bool>) {
        let new = if let Some(bit) = bits.next() {
            match self {
                Self::AllUnmarked => return,
                Self::AllMarked => {
                    let mut deeper = Self::AllMarked;
                    deeper.unmark(bits);
                    let mut other = Self::AllMarked;

                    if bit {
                        std::mem::swap(&mut deeper, &mut other);
                    }
                    Self::Branch(Box::new(deeper), Box::new(other))
                }
                Self::Branch(a, b) => {
                    let deeper = if bit { b } else { a };
                    deeper.unmark(bits);
                    return;
                }
            }
        } else {
            Self::AllUnmarked
        };
        _ = std::mem::replace(self, new);
    }

    /// check if a position in the tree dictated by an iterator is marked
    ///
    /// note that this does not consider branches that have all marked leaves
    /// to be marked. if this is undesirable, [`MarkTree::optimize`] can be
    /// used to clean these branches up
    pub fn is_marked(&self, mut bits: impl Iterator<Item = bool>) -> bool {
        match self {
            Self::AllUnmarked => false,
            Self::AllMarked => true,
            Self::Branch(a, b) => match bits.next() {
                Some(false) => a.is_marked(bits),
                Some(true) => b.is_marked(bits),
                None => false,
            },
        }
    }

    /// clean up branches that are entirely marked or unmarked
    pub fn optimize(&mut self) {
        if let Self::Branch(a, b) = self {
            a.optimize();
            b.optimize();
            if matches!(&**a, Self::AllUnmarked | Self::AllMarked) && a == b {
                // this is only reachable when cheap to clone since this is a leaf
                let new = (**a).clone();
                _ = std::mem::replace(self, new);
            }
        }
    }

    fn walk(&self, path: &mut Vec<bool>, callback: &mut impl FnMut(&Self, &[bool])) {
        callback(self, path);

        if let Self::Branch(a, b) = self {
            path.push(false);
            a.walk(path, callback);
            path.pop();
            path.push(true);
            b.walk(path, callback);
            path.pop();
        }
    }

    /// walk through the tree, calling a callback function on every node
    ///
    /// the callback function is passed the current node and the path taken to get there
    pub fn traverse(&self, mut callback: impl FnMut(&Self, &[bool])) {
        self.walk(&mut vec![], &mut callback);
    }

    /// create an [`Iterator`] over the tree
    ///
    /// the generic `T` is the type for expressing the path to the
    /// current node: since normal iterators do not allow returning
    /// references to themselves, we cannot give a slice like
    /// [`MarkTree::traverse`] does.
    pub fn iter<T>(&self) -> MarkTreeIter<'_, T>
    where
        T: ConvertBits,
    {
        MarkTreeIter::<T> {
            stack: vec![(self, TreePos::Root)],
            path: vec![],
            phantom: std::marker::PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
enum TreePos {
    Root,
    Branched { position: bool, level: usize },
}

/// an [`Iterator`] over [`MarkTree`]
///
/// this has the same behavior as [`MarkTree::traverse`], but
/// implemented without recursion
#[derive(Debug, Clone)]
#[must_use = "iterators do not do anything until consumed"]
pub struct MarkTreeIter<'a, T> {
    stack: Vec<(&'a MarkTree, TreePos)>,
    path: Vec<bool>,
    phantom: std::marker::PhantomData<T>,
}

impl<'a, T> Iterator for MarkTreeIter<'a, T>
where
    T: ConvertBits,
{
    type Item = (&'a MarkTree, T::Output);

    fn next(&mut self) -> Option<Self::Item> {
        let (tree, treepos) = self.stack.pop()?;
        if let TreePos::Branched { position, level } = treepos {
            self.path.truncate(level);
            self.path.push(position);
        }

        if let MarkTree::Branch(a, b) = tree {
            let level = if let TreePos::Branched { level, .. } = treepos {
                level + 1
            } else {
                0
            };
            self.stack.push((
                b,
                TreePos::Branched {
                    position: true,
                    level,
                },
            ));
            self.stack.push((
                a,
                TreePos::Branched {
                    position: false,
                    level,
                },
            ));
        }

        Some((tree, T::convert_bits(&self.path)))
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use crate::{BitRangeIter, ConvertBits, IpRange, MarkTree, ParseIpRangeError};
    use std::{net::IpAddr, str::FromStr};

    #[test]
    fn range_known() {
        let res: Vec<_> = BitRangeIter::from((b'h', 10)).map(u8::from).collect();
        assert_eq!(res, [0, 1, 1, 0, 1, 0, 0, 0, 0, 0]);

        let res: Vec<_> = BitRangeIter::from((1929445575u32, 7))
            .enumerate()
            .filter_map(|(n, b)| b.then_some(n))
            .collect();
        assert_eq!(res, [1, 2, 3, 6]);

        let mut iter = BitRangeIter::from((1u128, 128));
        assert_eq!(iter.clone().count(), 128);
        assert_eq!(iter.next(), Some(false));
        assert_eq!(iter.last(), Some(true));
    }

    #[test]
    fn tree_dedup() {
        let mut tree = MarkTree::new();
        tree.mark(BitRangeIter::from((0b11000000u8, 3)));
        assert!(tree.is_marked(BitRangeIter::from((0b11000000u8, 3))));
        assert!(tree.is_marked(BitRangeIter::from((0b11000000u8, 5))));
        assert!(!tree.is_marked(BitRangeIter::from((0b11000000u8, 2))));
        assert!(!tree.is_marked(BitRangeIter::from((0b10000000u8, 3))));
        let old = tree.clone();
        tree.mark(BitRangeIter::from((0b11010100u8, 6)));
        assert_eq!(old, tree);
        tree.mark(BitRangeIter::from((0b11000000u8, 2)));
        let mut new = MarkTree::new();
        new.mark(BitRangeIter::from((0b11000000u8, 2)));
        assert_eq!(tree, new);
    }

    #[test]
    fn tree_optimize() {
        let mut tree = MarkTree::new();
        tree.mark(BitRangeIter::from((0b01010000u8, 4)));
        tree.mark(BitRangeIter::from((0b01100000u8, 4)));
        tree.mark(BitRangeIter::from((0b01110000u8, 4)));
        let mut simple = MarkTree::new();
        simple.mark(BitRangeIter::from((0b01010000u8, 4)));
        simple.mark(BitRangeIter::from((0b01100000u8, 3)));

        assert_ne!(tree, simple);
        tree.optimize();
        assert_eq!(tree, simple);
    }

    #[test]
    fn tree_traverse() {
        let mut tree = MarkTree::new();
        tree.mark(BitRangeIter::from((0b1010000000000000u16, 4)));

        tree.traverse(|node, path| {
            if node == &MarkTree::AllMarked {
                assert_eq!(path, [true, false, true, false]);
            } else {
                assert_ne!(path, [true, false, true, false]);
            }
        });
    }

    #[test]
    fn tree_unmark() {
        let mut tree = MarkTree::new();
        tree.mark([true].into_iter());
        assert!(tree.is_marked([true].into_iter()));
        assert!(tree.is_marked([true, false].into_iter()));
        assert!(tree.is_marked([true, true].into_iter()));
        tree.unmark([true, true].into_iter());
        assert!(tree.is_marked([true, false].into_iter()));
        assert!(!tree.is_marked([true, true].into_iter()));
        assert!(!tree.is_marked([true, true, false].into_iter()));
    }

    #[test]
    fn iter_traverse() {
        let mut tree = MarkTree::new();
        tree.mark(BitRangeIter::from((7882829279673712640u64, 32)));
        tree.mark(BitRangeIter::from((7523377975159973992u64, 64)));
        let mut iter = tree.iter::<Vec<bool>>();
        tree.traverse(|tree, path| {
            let (itree, ipath) = iter.next().unwrap();
            assert_eq!(tree, itree);
            assert_eq!(path, ipath);
        });

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn range_str() {
        assert_eq!(
            IpRange::from_str("1.2.3.4").unwrap().into_parts(),
            (IpAddr::from_str("1.2.3.4").unwrap(), 32)
        );
        assert_eq!(
            IpRange::from_str("::ffff:1.2.3.4/127")
                .unwrap()
                .into_parts(),
            (IpAddr::from_str("1.2.3.4").unwrap(), 31)
        );
        assert_eq!(
            IpRange::from_str("fe80::/10").unwrap().to_string(),
            "fe80::/10"
        );
        assert_eq!(
            IpRange::from_str("127.6.2.1/33"),
            Err(ParseIpRangeError::MaskTooBig)
        );
        assert_eq!(
            IpRange::from_str("aaaa::/129"),
            Err(ParseIpRangeError::MaskTooBig)
        );
    }

    #[test]
    fn range_bits() {
        assert_eq!(
            IpRange::convert_bits(&[true, false, true])
                .unwrap()
                .to_string(),
            "a000::/3"
        );
        assert_eq!(
            IpRange::convert_bits(&[false; 128]).unwrap().to_string(),
            "::/128"
        );
        assert_eq!(
            IpRange::from_str("fa00::/8")
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
            [true, true, true, true, true, false, true, false]
        );
        assert_eq!(IpRange::convert_bits(&[false; 129]), None);
    }
}
