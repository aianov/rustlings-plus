use std::collections::HashMap;
use std::sync::Mutex;

struct ShardedMap<V> {
    shards: Vec<Mutex<HashMap<u64, V>>>,
}

impl<V: Default + Clone> ShardedMap<V> {
    fn new(shard_count: usize) -> Self {
        ShardedMap { shards: (0..shard_count).map(|_| Mutex::new(HashMap::new())).collect() }
    }

    fn shard_of(&self, key: u64) -> usize {
        let mixed = key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32;
        (mixed % self.shards.len() as u64) as usize
    }

    fn update<R>(&self, key: u64, f: impl FnOnce(&mut V) -> R) -> R {
        let mut shard = self.shards[self.shard_of(key)].lock().unwrap();
        f(shard.entry(key).or_default())
    }

    fn get(&self, key: u64) -> Option<V> {
        self.shards[self.shard_of(key)].lock().unwrap().get(&key).cloned()
    }

    fn len(&self) -> usize {
        self.shards.iter().map(|shard| shard.lock().unwrap().len()).sum()
    }
}

fn main() {
    let map: ShardedMap<u32> = ShardedMap::new(8);
    map.update(1, |v| *v += 5);
    println!("{:?} {}", map.get(1), map.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn behaves_like_a_map() {
        let map: ShardedMap<u32> = ShardedMap::new(8);
        for key in 0..100 {
            map.update(key, |value| *value += key as u32);
        }
        map.update(5, |value| *value += 1);
        assert_eq!(map.len(), 100, "сто разных ключей");
        assert_eq!(map.get(5), Some(6), "значение накапливается");
        assert_eq!(map.get(500), None, "чужого ключа нет");
    }

    #[test]
    fn keys_are_spread_over_all_shards() {
        let map: ShardedMap<u32> = ShardedMap::new(8);
        let mut counts = [0usize; 8];
        for key in 0..1000 {
            counts[map.shard_of(key)] += 1;
        }
        assert!(counts.iter().all(|count| *count > 40), "ключи легли неравномерно: {counts:?}");
    }

    #[test]
    fn regular_keys_do_not_pile_up() {
        let map: ShardedMap<u32> = ShardedMap::new(16);
        let mut counts = [0usize; 16];
        for i in 0..1600u64 {
            counts[map.shard_of(i * 64)] += 1;
        }
        let busiest = counts.iter().max().unwrap();
        assert!(*busiest < 400, "ключи с шагом 64 легли в считаные шарды: {counts:?}; хеш не должен зависеть от закономерности ключей");
    }

    fn peak_inside(map: &Arc<ShardedMap<u32>>, keys: &[u64]) -> usize {
        let inside = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let threads: Vec<_> = keys
            .iter()
            .map(|key| {
                let (map, inside, peak, key) = (Arc::clone(map), Arc::clone(&inside), Arc::clone(&peak), *key);
                std::thread::spawn(move || {
                    map.update(key, |_| {
                        let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(60));
                        inside.fetch_sub(1, Ordering::SeqCst);
                    })
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        peak.load(Ordering::SeqCst)
    }

    #[test]
    fn different_shards_work_in_parallel() {
        let map: Arc<ShardedMap<u32>> = Arc::new(ShardedMap::new(16));
        let mut keys = Vec::new();
        let mut used = std::collections::HashSet::new();
        for key in 0..10_000 {
            if used.insert(map.shard_of(key)) {
                keys.push(key);
            }
            if keys.len() == 6 {
                break;
            }
        }
        assert_eq!(keys.len(), 6, "не нашлось шести ключей из разных шардов");
        let peak = peak_inside(&map, &keys);
        assert!(peak >= 4, "обновления ключей из разных шардов шли по очереди (одновременно не больше {peak}): замок должен быть свой у каждого шарда");
    }

    #[test]
    fn same_shard_is_serialized() {
        let map: Arc<ShardedMap<u32>> = Arc::new(ShardedMap::new(16));
        let target = map.shard_of(0);
        let keys: Vec<u64> = (0..10_000).filter(|key| map.shard_of(*key) == target).take(4).collect();
        assert_eq!(peak_inside(&map, &keys), 1, "ключи одного шарда не должны обновляться одновременно");
    }
}
