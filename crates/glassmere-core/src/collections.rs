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
