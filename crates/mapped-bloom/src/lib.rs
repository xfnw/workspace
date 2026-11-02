// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: CC0-1.0

use std::{
    fs::File,
    hash::{DefaultHasher, Hash, Hasher},
    io,
};

use memmap2::{MmapMut, MmapOptions};

/// a bloom filter that is memory mapped onto a file
pub struct BloomFilter {
    _file: File,
    map: MmapMut,
    num_hashes: u8,
}

impl BloomFilter {
    /// create a new [`BloomFilter`] from a file
    ///
    /// the file should be both readable and writeable
    ///
    /// # Errors
    /// this will error if the file cannot be locked, the `SIZE` const
    /// parameter is an unsuitable size, or if the map cannot be created
    pub fn new(file: File, num_hashes: u8, size: u64) -> io::Result<Self> {
        if size < 1 {
            return Err(io::Error::other("size must be at least 1"));
        }

        file.try_lock()?;
        file.set_len(size)?;
        let mut map = MmapOptions::new();
        let map = map.len(size.try_into().map_err(io::Error::other)?);
        let map = unsafe { map.map_mut(&file)? };

        Ok(Self {
            _file: file,
            map,
            num_hashes,
        })
    }

    #[expect(clippy::cast_possible_truncation)]
    fn get_bit(&self, bit: u64) -> bool {
        let (byte, flag) = split_bit(bit);
        self.map[(byte % self.map.len() as u64) as usize] & flag != 0
    }

    #[expect(clippy::cast_possible_truncation)]
    fn set_bit(&mut self, bit: u64) {
        let (byte, flag) = split_bit(bit);
        let size = self.map.len() as u64;
        self.map[(byte % size) as usize] |= flag;
    }

    /// returns `true` if the bloom filter possibly contains value
    pub fn contains<H: Hash>(&self, value: H) -> bool {
        Hashes::<H>::new(value, self.num_hashes).all(|hash| self.get_bit(hash))
    }

    /// add a value to the bloom filter
    pub fn insert<H: Hash>(&mut self, value: H) {
        Hashes::<H>::new(value, self.num_hashes).for_each(|hash| self.set_bit(hash));
    }

    /// flush mapping to disk, blocking until completion
    ///
    /// # Errors
    /// returns an error if changes might not have been saved
    pub fn flush(&self) -> io::Result<()> {
        self.map.flush()
    }

    /// flush mapping to disk without waiting
    ///
    /// this is a no-op on linux since 2.6.19
    ///
    /// # Errors
    /// returns an error if saving changes failed to be scheduled
    pub fn flush_async(&self) -> io::Result<()> {
        self.map.flush_async()
    }
}

struct Hashes<H: Hash> {
    data: H,
    i: u8,
    num: u8,
}

impl<H: Hash> Hashes<H> {
    fn new(data: H, num: u8) -> Self {
        Self { data, i: 0, num }
    }
}

impl<H: Hash> Iterator for Hashes<H> {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.i == self.num {
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
        let mut bloom = BloomFilter::new(file, 4, 8192)?;

        assert!(!bloom.contains("meow"));

        bloom.insert("meow");

        assert!(bloom.contains("meow"));
        assert!(!bloom.contains("woem"));

        Ok(())
    }
}
