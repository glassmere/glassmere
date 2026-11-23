//! Hash collections: std's with `std`, hashbrown's without, so that fields
//! and signatures that name them keep std's types for everyone who has it.

use core::hash::Hasher;

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

/// A hasher for the kernel's own keys: node identities, call sites and the
/// integers a widget key is made of. They are looked up several times for
/// every widget of every frame, and std's default hasher, which is built to
/// withstand keys chosen by an attacker, costs more than the rest of the
/// lookup. Nothing outside chooses these keys. It is the multiply-and-rotate
/// hash rustc uses. The names of widgets are chosen by the application, from
/// data that others can choose, so their bytes go through [`KeyState`], which
/// uses this hash only for the integers.
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
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
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

/// The seed and odd multiplier of the keyed hash of names, drawn once per
/// process, so a table is made without drawing any randomness.
#[cfg(feature = "std")]
fn draw_seed() -> (u64, u64) {
    use core::hash::BuildHasher;
    use std::collections::hash_map::RandomState;
    use std::sync::OnceLock;

    static SEED: OnceLock<(u64, u64)> = OnceLock::new();
    *SEED.get_or_init(|| {
        let random = RandomState::new();
        (
            random.hash_one(0x6b65_795f_7365_6564_u64),
            random.hash_one(0x6b65_795f_6d75_6c74_u64) | 1,
        )
    })
}

/// The seed and odd multiplier of the keyed hash of names, drawn for every
/// state: there is no place to keep one per process without std.
#[cfg(not(feature = "std"))]
fn draw_seed() -> (u64, u64) {
    use core::hash::BuildHasher;

    let random = hashbrown::DefaultHashBuilder::default();
    (
        random.hash_one(0x6b65_795f_7365_6564_u64),
        random.hash_one(0x6b65_795f_6d75_6c74_u64) | 1,
    )
}

/// The hasher state of the tables keyed by widget keys. A widget name can be
/// chosen by whoever supplies the data the application shows, so its bytes
/// are folded with the hash glassmere-text uses for its keys: each word goes
/// into the state as the two halves of the product of the state and a
/// multiplier, xored together. The start state and the multiplier must be
/// random and secret, and the multiplier odd: with a known seed, or an even
/// multiplier, names can be crafted that collide in every bucket of a table.
/// The integers a call site is made of are mixed as [`FastHasher`] mixes
/// them.
#[derive(Clone, Copy)]
pub(crate) struct KeyState {
    state: u64,
    mul: u64,
}

impl KeyState {
    /// A state with the given start state and multiplier, which must be odd.
    #[cfg(test)]
    pub(crate) fn with_seed(state: u64, mul: u64) -> Self {
        Self { state, mul }
    }
}

impl Default for KeyState {
    fn default() -> Self {
        let (state, mul) = draw_seed();
        Self { state, mul }
    }
}

/// The hasher of a [`KeyState`].
pub(crate) struct KeyHasher {
    fast: FastHasher,
    state: u64,
    mul: u64,
}

/// Folds a word into the state of a [`KeyHasher`] write.
fn fold(state: u64, mul: u64, word: u64) -> u64 {
    let product = u128::from(state ^ word) * u128::from(mul);
    (product as u64) ^ ((product >> 64) as u64)
}

impl core::hash::BuildHasher for KeyState {
    type Hasher = KeyHasher;

    fn build_hasher(&self) -> KeyHasher {
        KeyHasher {
            fast: FastHasher::default(),
            state: self.state,
            mul: self.mul,
        }
    }
}

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.fast.finish()
    }

    fn write(&mut self, bytes: &[u8]) {
        // Every write starts from the seed, so that the hash of a name does
        // not depend on what was written before it.
        let mut state = self.state;
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            state = fold(state, self.mul, u64::from_le_bytes(*word));
        }
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            state = fold(state, self.mul, u64::from_le_bytes(word));
        }
        // Without the length, "a" and "a\0" would give the same words.
        state = fold(state, self.mul, bytes.len() as u64);
        self.fast.write_u64(state);
    }

    fn write_u8(&mut self, value: u8) {
        self.fast.write_u8(value);
    }

    fn write_u32(&mut self, value: u32) {
        self.fast.write_u32(value);
    }

    fn write_u64(&mut self, value: u64) {
        self.fast.write_u64(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.fast.write_usize(value);
    }
}

/// A map over widget keys; see [`KeyState`].
pub(crate) type KeyMap<V> = HashMap<crate::ui::WidgetKey, V, KeyState>;

pub(crate) type FastState = core::hash::BuildHasherDefault<FastHasher>;
/// A map over the kernel's own keys; see [`FastHasher`].
pub(crate) type FastMap<K, V> = HashMap<K, V, FastState>;
/// A set over the kernel's own keys; see [`FastHasher`].
pub(crate) type FastSet<K> = HashSet<K, FastState>;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ui::WidgetKey;
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
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

    const K: u64 = FastHasher::SEED;

    fn step(state: u64, word: u64) -> u64 {
        (state.rotate_left(5) ^ word).wrapping_mul(K)
    }

    /// What `FastHasher` makes of a name hashed as a derived `Hash` did,
    /// which is how the hostile sets below are aimed.
    fn fast_hash_of(name: &[u8]) -> u64 {
        let mut hasher = FastHasher::default();
        hasher.write_usize(0);
        hasher.write(name);
        hasher.write_u8(0xff);
        hasher.finish()
    }

    fn counted(mut n: u64, alphabet: &[u8], out: &mut [u8]) {
        for byte in out {
            *byte = alphabet[(n % alphabet.len() as u64) as usize];
            n /= alphabet.len() as u64;
        }
    }

    fn printable(word: u64) -> bool {
        word.to_le_bytes()
            .iter()
            .all(|byte| (0x21..0x7f).contains(byte))
    }

    /// Names that all have one `FastHasher` hash.
    pub(crate) fn one_hash(count: usize) -> Vec<String> {
        let after = step(0, 0);
        let aim = step(after, u64::from_le_bytes(*b"key-key-")).rotate_left(5)
            ^ u64::from_le_bytes(*b"hostile!");
        let mut names = Vec::with_capacity(count);
        let mut n = 0u64;
        while names.len() < count {
            let mut first = [0u8; 8];
            counted(n, b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", &mut first);
            n += 1;
            let second = step(after, u64::from_le_bytes(first)).rotate_left(5) ^ aim;
            if printable(second) {
                let mut name = first.to_vec();
                name.extend_from_slice(&second.to_le_bytes());
                names.push(String::from_utf8(name).unwrap());
            }
        }
        names
    }

    /// Names that pair up under any start state and multiplier: the two
    /// halves of a pair differ in one bit of the first word that the
    /// multiplication shifts out, and in two bits of the second.
    pub(crate) fn pairs(count: usize) -> Vec<String> {
        let mut names = Vec::with_capacity(count + 8);
        let mut base = 0u64;
        while names.len() < count {
            for eighth in *b"1q" {
                for ninth in *b"bjrz" {
                    let mut name = [0u8; 9];
                    counted(base, b"klmnopqrstuvwxyz", &mut name[..7]);
                    name[7] = eighth;
                    name[8] = ninth;
                    names.push(String::from_utf8(name.to_vec()).unwrap());
                }
            }
            base += 1;
        }
        names.truncate(count);
        names
    }

    /// Names whose `FastHasher` hashes differ but share the low 20 and the
    /// top 7 bits.
    pub(crate) fn one_bucket(count: usize) -> Vec<String> {
        let want = fast_hash_of(b"zzVERIFY");
        let mut names = Vec::with_capacity(count);
        let mut n = 0u64;
        while names.len() < count {
            let mut name = *b"zz______";
            counted(
                n,
                b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz",
                &mut name[2..],
            );
            n += 1;
            let got = fast_hash_of(&name);
            if (got ^ want) & 0xf_ffff == 0 && (got ^ want) >> 57 == 0 {
                names.push(String::from_utf8(name.to_vec()).unwrap());
            }
        }
        names
    }

    /// A name and the same name followed by one to seven zero bytes.
    pub(crate) fn padded(count: usize) -> Vec<String> {
        let mut names = Vec::with_capacity(count + 8);
        let mut base = 0u64;
        while names.len() < count {
            let mut stem = [0u8; 9];
            counted(base, b"abcdefghijklmnopqrstuvwxyz", &mut stem);
            for zeros in 0..8 {
                let mut name = stem.to_vec();
                name.extend(core::iter::repeat_n(0u8, zeros));
                names.push(String::from_utf8(name).unwrap());
            }
            base += 1;
        }
        names.truncate(count);
        names
    }

    #[test]
    fn the_hostile_sets_hash_alike_under_the_fast_hash() {
        let distinct = |names: &[String]| -> usize {
            let hashes: FastSet<u64> = names.iter().map(|n| fast_hash_of(n.as_bytes())).collect();
            hashes.len()
        };
        assert_eq!(distinct(&one_hash(512)), 1);
        assert_eq!(distinct(&pairs(512)), 256);
        assert_eq!(distinct(&padded(512)), 64);
        let bucket = one_bucket(512);
        assert_eq!(distinct(&bucket), 512);
        let low: FastSet<u64> = bucket
            .iter()
            .map(|n| fast_hash_of(n.as_bytes()) & 0xf_ffff)
            .collect();
        assert_eq!(low.len(), 1);
    }

    /// A small deterministic generator (splitmix64) of seed pairs.
    fn seeds(count: usize) -> Vec<(u64, u64)> {
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        };
        (0..count).map(|_| (next(), next() | 1)).collect()
    }

    fn assert_spread(state: &KeyState, label: &str, names: &[String]) {
        let hashes: Vec<u64> = names
            .iter()
            .map(|name| state.hash_one(WidgetKey::Named(name.clone())))
            .collect();
        let all: FastSet<u64> = hashes.iter().copied().collect();
        assert_eq!(all.len(), 10_000, "{label}");
        let low: FastSet<u64> = hashes.iter().map(|hash| hash & 0xfff).collect();
        assert!(low.len() > 2500, "{label}: {}", low.len());
    }

    #[test]
    fn names_that_collide_under_the_fast_hash_are_spread_by_the_key_state() {
        let rows: Vec<String> = (0..10_000).map(|n| format!("row-{n}")).collect();
        let ids: Vec<String> = (100_000_000..100_010_000u32)
            .map(|n| n.to_string())
            .collect();
        let sets = [
            ("one hash", one_hash(10_000)),
            ("pairs", pairs(10_000)),
            ("one bucket", one_bucket(10_000)),
            ("padded", padded(10_000)),
            ("rows", rows),
            ("ids", ids),
        ];
        for (state, mul) in seeds(200) {
            let state = KeyState::with_seed(state, mul);
            for (label, names) in &sets {
                assert_spread(&state, label, names);
            }
        }
        let state = KeyState::default();
        for (label, names) in &sets {
            assert_spread(&state, label, names);
        }
    }

    #[test]
    fn row_names_numbered_past_fifty_thousand_do_not_pair_up() {
        let state = KeyState::default();
        let hashes: FastSet<u64> = (0..100_000)
            .map(|n| state.hash_one(WidgetKey::Named(format!("row-{n}"))))
            .collect();
        assert_eq!(hashes.len(), 100_000);
    }

    #[test]
    fn two_seeds_hash_a_name_differently() {
        let key = WidgetKey::Named("a".into());
        let pairs = seeds(2);
        let first = KeyState::with_seed(pairs[0].0, pairs[0].1);
        let second = KeyState::with_seed(pairs[1].0, pairs[1].1);
        assert_ne!(first.hash_one(&key), second.hash_one(&key));
        assert_eq!(first.hash_one(&key), first.hash_one(&key));
    }

    #[test]
    fn the_multiplier_of_a_default_state_is_odd() {
        for _ in 0..16 {
            assert_eq!(KeyState::default().mul & 1, 1);
        }
    }
}
