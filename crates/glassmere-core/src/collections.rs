//! Hash collections: std's with `std`, hashbrown's without, so that fields
//! and signatures that name them keep std's types for everyone who has it.

#[cfg(feature = "std")]
pub(crate) use std::collections::{HashMap, HashSet};

#[cfg(not(feature = "std"))]
pub(crate) use hashbrown::{HashMap, HashSet};

/// A hasher that gives the same hash for the same value in every run and on
/// every thread, which a hash that is kept from one frame to the next needs.
#[cfg(feature = "std")]
pub(crate) type StableHasher = std::collections::hash_map::DefaultHasher;

#[cfg(not(feature = "std"))]
pub(crate) struct StableHasher(u64);

#[cfg(not(feature = "std"))]
impl StableHasher {
    pub(crate) fn new() -> Self {
        // FNV-1a, 64 bit.
        Self(0xcbf2_9ce4_8422_2325)
    }
}

#[cfg(not(feature = "std"))]
impl core::hash::Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

/// A hasher for the kernel's own keys: node identities, widget keys and
/// call sites. They are looked up several times for every widget of every
/// frame, and std's default hasher, which is built to withstand keys chosen
/// by an attacker, costs more than the rest of the lookup. Nothing outside
/// chooses these keys. It is the multiply-and-rotate hash rustc uses.
#[derive(Clone, Copy, Default)]
pub(crate) struct FastHasher(u64);

impl FastHasher {
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }
}

impl core::hash::Hasher for FastHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            self.add(u64::from_le_bytes(chunk.try_into().expect("eight bytes")));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }
}

pub(crate) type FastState = core::hash::BuildHasherDefault<FastHasher>;
/// A map over the kernel's own keys; see [`FastHasher`].
pub(crate) type FastMap<K, V> = HashMap<K, V, FastState>;
/// A set over the kernel's own keys; see [`FastHasher`].
pub(crate) type FastSet<K> = HashSet<K, FastState>;

#[cfg(test)]
mod tests {
    use super::*;
    use core::hash::{BuildHasher, Hash};

    #[test]
    fn the_fast_hash_tells_keys_apart_and_spreads_counted_ones() {
        let hash = |value: &dyn Fn(&mut FastHasher)| {
            let mut hasher = FastState::default().build_hasher();
            value(&mut hasher);
            core::hash::Hasher::finish(&hasher)
        };
        // Strings that differ only in their tail, or only in length.
        let texts = ["", "a", "ab", "abcdefgh", "abcdefghi", "abcdefgh\0"];
        let hashes: FastSet<u64> = texts.iter().map(|text| hash(&|h| text.hash(h))).collect();
        assert_eq!(hashes.len(), texts.len());
        // Identities are counted up from one: their hashes must not share
        // the low bits a table indexes by, nor the high bits it tags with.
        let ids: alloc::vec::Vec<u64> = (1..=4096u64).map(|id| hash(&|h| id.hash(h))).collect();
        let low: FastSet<u64> = ids.iter().map(|hash| hash & 0xfff).collect();
        let high: FastSet<u64> = ids.iter().map(|hash| hash >> 57).collect();
        assert!(low.len() > 2500, "{}", low.len());
        assert_eq!(high.len(), 128);
        let mut map: FastMap<u64, u64> = FastMap::default();
        for id in 1..=10_000u64 {
            map.insert(id, id * 2);
        }
        assert!((1..=10_000u64).all(|id| map[&id] == id * 2));
    }
}
