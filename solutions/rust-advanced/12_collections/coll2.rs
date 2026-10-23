use std::collections::HashMap;
use std::hash::Hash;

pub fn get_or_load<K: Hash + Eq + Copy, V>(cache: &mut HashMap<K, V>, key: K, load: impl FnOnce(K) -> V) -> &V {
    cache.entry(key).or_insert_with(|| load(key))
}

pub fn touch<K: Hash + Eq>(seen: &mut HashMap<K, u32>, key: K) -> u32 {
    let n = seen.entry(key).or_insert(0);
    *n += 1;
    *n
}

fn main() {
    let mut cache = HashMap::new();
    println!("{}", get_or_load(&mut cache, 1_u64, |k| k * 2));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::hash::Hasher;

    thread_local! {
        static HASHES: Cell<usize> = const { Cell::new(0) };
    }

    fn hashes() -> usize {
        HASHES.with(Cell::get)
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Key(u64);

    impl Hash for Key {
        fn hash<H: Hasher>(&self, state: &mut H) {
            HASHES.with(|c| c.set(c.get() + 1));
            self.0.hash(state);
        }
    }

    #[test]
    fn a_hit_hashes_the_key_once() {
        let mut cache = HashMap::with_capacity(16);
        cache.insert(Key(1), "one".to_string());
        let before = hashes();
        assert_eq!(get_or_load(&mut cache, Key(1), |_| "never".to_string()), "one");
        assert_eq!(hashes() - before, 1, "попадание в кэш: ключ хешируется один раз, а не по разу на проверку и на чтение");
    }

    #[test]
    fn a_miss_hashes_the_key_once_and_loads_once() {
        let mut cache = HashMap::with_capacity(16);
        let loads = Cell::new(0);
        let before = hashes();
        let got = get_or_load(&mut cache, Key(2), |k| {
            loads.set(loads.get() + 1);
            format!("post {}", k.0)
        });
        assert_eq!(got, "post 2");
        assert_eq!(hashes() - before, 1, "промах: поиск и вставка должны идти одним проходом");
        assert_eq!(loads.get(), 1);
        assert_eq!(get_or_load(&mut cache, Key(2), |_| unreachable!("значение уже загружено")), "post 2");
    }

    #[test]
    fn touch_counts_with_one_hash_per_call() {
        let mut seen = HashMap::with_capacity(16);
        let before = hashes();
        assert_eq!(touch(&mut seen, Key(3)), 1);
        assert_eq!(hashes() - before, 1, "первое обращение: один хеш");
        let before = hashes();
        assert_eq!(touch(&mut seen, Key(3)), 2);
        assert_eq!(touch(&mut seen, Key(3)), 3);
        assert_eq!(hashes() - before, 2, "повторные обращения: по одному хешу на вызов");
    }
}
