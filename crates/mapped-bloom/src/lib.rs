// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: CC0-1.0

use std::{
    fs::File,
    hash::{DefaultHasher, Hash, Hasher},
    io,
};

use memmap2::{MmapMut, MmapOptions};

pub struct BloomFilter<const HASHES: u8, const SIZE: u64> {
    _file: File,
    map: MmapMut,
}

impl<const HASHES: u8, const SIZE: u64> BloomFilter<HASHES, SIZE> {
    pub fn new(file: File) -> io::Result<Self> {
        file.try_lock()?;
        file.set_len(SIZE)?;
        let mut map = MmapOptions::new();
        let map = map.len(SIZE.try_into().map_err(io::Error::other)?);
        let map = unsafe { map.map_mut(&file)? };
        Ok(Self { _file: file, map })
    }

    #[expect(clippy::cast_possible_truncation)]
    fn get_bit(&self, bit: u64) -> bool {
        let (byte, flag) = split_bit(bit);
        self.map[(byte % SIZE) as usize] & flag != 0
    }

    #[expect(clippy::cast_possible_truncation)]
    fn set_bit(&mut self, bit: u64) {
        let (byte, flag) = split_bit(bit);
        self.map[(byte % SIZE) as usize] |= flag;
    }

    pub fn contains<H: Hash>(&self, value: H) -> bool {
        Hashes::<HASHES, H>::new(value).all(|hash| self.get_bit(hash))
    }

    pub fn insert<H: Hash>(&mut self, value: H) {
        Hashes::<HASHES, H>::new(value).for_each(|hash| self.set_bit(hash));
    }

    pub fn flush(&self) -> io::Result<()> {
        self.map.flush()
    }

    pub fn flush_async(&self) -> io::Result<()> {
        self.map.flush_async()
    }
}

struct Hashes<const HASHES: u8, H: Hash> {
    data: H,
    i: u8,
}

impl<const HASHES: u8, H: Hash> Hashes<HASHES, H> {
    fn new(data: H) -> Self {
        Self { data, i: 0 }
    }
}

impl<const HASHES: u8, H: Hash> Iterator for Hashes<HASHES, H> {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.i == HASHES {
            return None;
        }
        let mut hasher = DefaultHasher::new();
        hasher.write_u8(self.i);
        self.data.hash(&mut hasher);
        self.i += 1;
        Some(hasher.finish())
    }
}

fn split_bit(bit: u64) -> (u64, u8) {
    (bit >> 3, 1 << (bit & 7))
}

#[cfg(test)]
mod tests {
    use super::BloomFilter;
    use std::io;

    #[test]
    fn it_works() -> Result<(), io::Error> {
        let file = tempfile::tempfile()?;
        let mut bloom = BloomFilter::<4, 8192>::new(file)?;

        assert!(!bloom.contains("meow"));

        bloom.insert("meow");

        assert!(bloom.contains("meow"));
        assert!(!bloom.contains("woem"));

        Ok(())
    }
}
