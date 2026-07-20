// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
};

// format: directory entries just concatenated together
#[derive(Debug, PartialEq, Eq)]
pub struct Directory<const N: usize> {
    pub entries: Vec<DirectoryEntry<N>>,
}

impl<const N: usize> Directory<N> {
    #[must_use]
    pub fn parse(inp: &[u8]) -> Option<Self> {
        let mut rest = inp;
        let mut entries = vec![];

        while let Some((entry, next_rest)) = DirectoryEntry::parse(rest) {
            entries.push(entry);
            rest = next_rest;
        }

        if !rest.is_empty() {
            return None;
        }

        Some(Self { entries })
    }

    #[must_use]
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = vec![];

        for entry in &self.entries {
            entry.serialize(&mut out);
        }

        out
    }
}

// format:
// N bytes      - hash
// big endian 16 bits (
//   3 bits  - reserved
//   1 bit   - is directory
//   12 bits - name length in bytes
// )
// 0-4095 bytes - name
#[derive(Debug, PartialEq, Eq)]
pub struct DirectoryEntry<const N: usize> {
    pub hash: [u8; N],
    pub kind: DirectoryEntryKind,
    pub name: OsString,
}

impl<const N: usize> DirectoryEntry<N> {
    fn parse(inp: &[u8]) -> Option<(Self, &[u8])> {
        let (hash, rest) = parse_hash(inp)?;
        let (kind, length, rest) = parse_flags(rest)?;
        let (name, rest) = parse_name(rest, length)?;

        let dir = Self { hash, kind, name };
        Some((dir, rest))
    }

    fn serialize(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.hash);

        let length = self.name.len();
        assert!(length < 4096);
        #[expect(clippy::cast_possible_truncation)]
        let flags = match self.kind {
            DirectoryEntryKind::File => 0,
            DirectoryEntryKind::Subdirectory => 1 << 12,
        } | length as u16;
        out.extend_from_slice(&flags.to_be_bytes());

        out.extend_from_slice(self.name.as_bytes());
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DirectoryEntryKind {
    File,
    Subdirectory,
}

fn parse_hash<const N: usize>(inp: &[u8]) -> Option<([u8; N], &[u8])> {
    inp.split_first_chunk::<N>().map(|(&h, r)| (h, r))
}

fn parse_flags(inp: &[u8]) -> Option<(DirectoryEntryKind, u16, &[u8])> {
    let (flags, rest) = inp.split_first_chunk::<2>()?;
    let flags = u16::from_be_bytes(*flags);

    let kind = if flags & (1 << 12) == 0 {
        DirectoryEntryKind::File
    } else {
        DirectoryEntryKind::Subdirectory
    };
    let length = flags & 4095;

    Some((kind, length, rest))
}

fn parse_name(inp: &[u8], length: u16) -> Option<(OsString, &[u8])> {
    assert!(length < 4096);
    let (name, rest) = inp.split_at_checked(length as usize)?;
    Some((OsStr::from_bytes(name).to_os_string(), rest))
}

#[test]
fn round_trip() {
    let dir = Directory {
        entries: vec![
            DirectoryEntry {
                hash: *b"abcdefghijklmnop",
                kind: DirectoryEntryKind::File,
                name: "meowmeow".to_string().into(),
            },
            DirectoryEntry {
                hash: *b"hhhhhhhhhhhhhhhh",
                kind: DirectoryEntryKind::Subdirectory,
                name: "the letter h".to_string().into(),
            },
            DirectoryEntry {
                hash: *b"barkbarkbarkbark",
                kind: DirectoryEntryKind::File,
                name: "woof".to_string().into(),
            },
        ],
    };
    let serialized = dir.serialize();
    let parsed = Directory::parse(&serialized).unwrap();
    assert_eq!(dir, parsed);
}
