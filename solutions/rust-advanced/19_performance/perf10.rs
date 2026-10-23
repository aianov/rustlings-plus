use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

pub struct Fnv64 {
    state: u64,
}

impl Default for Fnv64 {
    fn default() -> Self {
        Fnv64 { state: OFFSET }
    }
}

impl Hasher for Fnv64 {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= u64::from(byte);
            self.state = self.state.wrapping_mul(PRIME);
        }
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

#[derive(Clone, Default)]
pub struct FnvBuilder;

impl BuildHasher for FnvBuilder {
    type Hasher = Fnv64;

    fn build_hasher(&self) -> Fnv64 {
        Fnv64::default()
    }
}

pub type FnvMap<K, V> = HashMap<K, V, FnvBuilder>;

pub fn new_map<K, V>() -> FnvMap<K, V> {
    HashMap::with_hasher(FnvBuilder)
}

fn main() {
    let mut counts: FnvMap<u32, u32> = new_map();
    *counts.entry(7).or_default() += 1;
    println!("{counts:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv(bytes: &[u8]) -> u64 {
        let mut hasher = Fnv64::default();
        hasher.write(bytes);
        hasher.finish()
    }

    #[test]
    fn known_vectors() {
        assert_eq!(fnv(b""), 0xcbf2_9ce4_8422_2325, "хеш пустых данных это начальное состояние");
        assert_eq!(fnv(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn writes_can_be_split() {
        let mut parts = Fnv64::default();
        parts.write(b"foo");
        parts.write(b"bar");
        assert_eq!(parts.finish(), fnv(b"foobar"), "хеш по кускам равен хешу целиком");
        assert_eq!(parts.finish(), parts.finish(), "finish не меняет состояние");
    }

    #[test]
    fn keys_are_hashed_through_the_trait() {
        let builder = FnvBuilder;
        assert_eq!(builder.hash_one(1u32), 0xad2a_ca77_4798_5764, "u32 пишется четырьмя байтами little endian");
        assert_eq!(builder.hash_one("foobar"), 0x3452_4ba7_168a_2c15, "str пишет байты и ещё один 0xff в конце");
        assert_eq!(builder.hash_one(258u64), 0x216b_0ab9_ec24_fb2c);
        assert_ne!(builder.hash_one(1u32), builder.hash_one(2u32));
    }

    #[test]
    fn map_works_and_is_deterministic() {
        let mut a: FnvMap<u32, u32> = new_map();
        let mut b: FnvMap<u32, u32> = new_map();
        for n in 0..1_000 {
            a.insert(n, n * 2);
            b.insert(n, n * 2);
        }
        assert_eq!(a.len(), 1_000);
        assert_eq!(a.get(&777), Some(&1_554));
        assert_eq!(a.get(&5_000), None);
        let order_a: Vec<u32> = a.keys().copied().collect();
        let order_b: Vec<u32> = b.keys().copied().collect();
        assert_eq!(order_a, order_b, "с фиксированным хешером порядок обхода повторяется от запуска к запуску");
    }

    #[test]
    fn string_keys_work() {
        let mut m: FnvMap<String, usize> = new_map();
        for word in "раз два три два раз два".split(' ') {
            *m.entry(word.to_string()).or_default() += 1;
        }
        assert_eq!((m["раз"], m["два"], m["три"]), (2, 3, 1));
    }
}
