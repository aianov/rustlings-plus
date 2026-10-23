
fn main() {}

pub fn shard_of(key: &str, shards: u32) -> u32 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in key.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    (h % u64::from(shards)) as u32
}

pub fn moved(keys: &[String], from: u32, to: u32) -> usize {
    keys.iter().filter(|k| shard_of(k, from) != shard_of(k, to)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("profile:{i}")).collect()
    }

    #[test]
    fn the_hash_is_the_agreed_one() {
        assert_eq!(shard_of("", 1000), (0xcbf29ce484222325u64 % 1000) as u32, "пустой ключ это начальное значение хеша");
        assert_eq!(shard_of("a", 1_000_000), (0xaf63dc4c8601ec8cu64 % 1_000_000) as u32, "FNV-1a 64 от строки «a»");
        assert_eq!(shard_of("user:1", 7), shard_of("user:1", 7));
    }

    #[test]
    fn shards_are_evenly_loaded() {
        let mut counts = [0usize; 10];
        for k in keys(10_000) {
            counts[shard_of(&k, 10) as usize] += 1;
        }
        assert!(counts.iter().all(|c| (850..=1150).contains(c)), "шарды нагружены неровно: {counts:?}");
    }

    #[test]
    fn adding_one_node_moves_almost_everything() {
        let ks = keys(10_000);
        let m = moved(&ks, 10, 11);
        assert!(m > 8_000, "при переходе с 10 на 11 узлов переехало только {m} из 10 000 ключей; по модулю переезжает около 90%");
        assert!(m < 10_000, "не все ключи переезжают: каждые 110 ключей остаются на месте");
        assert_eq!(moved(&ks, 10, 10), 0);
    }

    #[test]
    fn doubling_the_nodes_moves_about_half() {
        let ks = keys(10_000);
        let m = moved(&ks, 10, 20);
        assert!((4_000..6_000).contains(&m), "при удвоении переезжает примерно половина, а получилось {m}");
    }
}
