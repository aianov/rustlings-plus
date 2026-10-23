// Добавили одиннадцатый узел кэша, и почти все ключи разом оказались «не на своём месте»: ответы кэша пропали, база приняла на себя
// всю нагрузку и легла. А ведь честное деление на 11 узлов требует переезда лишь одной одиннадцатой ключей. Нужно такое
// распределение, где добавление или удаление узла трогает только соседей. Это кольцо: и ключи, и узлы получают место на окружности
// (хеш), ключ принадлежит первому узлу по часовой стрелке. Чтобы узлы делили кольцо поровну, каждому даётся много точек (виртуальных узлов).
// Цель: `Ring` с `add`, `remove` и `node_for`. Узел занимает `vnodes` точек `hash("<узел>#<номер>")`, ключ принадлежит узлу первой точки,
// не меньшей хеша ключа (после последней точки обход возвращается к первой). Хеш `hash64` уже готов.


use std::collections::BTreeMap;

fn main() {}

/// FNV-1a 64 бита с финальным перемешиванием (иначе похожие строки лежали бы на кольце кучками).
pub fn hash64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h
}

pub struct Ring {
    vnodes: usize,
    nodes: Vec<String>,
}

impl Ring {
    pub fn new(nodes: &[&str], vnodes: usize) -> Ring {
        let _ = BTreeMap::<u64, ()>::new();
        Ring { vnodes, nodes: nodes.iter().map(|n| n.to_string()).collect() }
    }

    pub fn add(&mut self, node: &str) {
        self.nodes.push(node.to_string());
    }

    pub fn remove(&mut self, node: &str) {
        self.nodes.retain(|n| n != node);
    }

    pub fn node_for(&self, key: &str) -> Option<&str> {
        if self.nodes.is_empty() {
            return None;
        }
        Some(&self.nodes[(hash64(key) % self.nodes.len() as u64) as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn keys(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("profile:{i}")).collect()
    }

    fn assignment(ring: &Ring, ks: &[String]) -> Vec<String> {
        ks.iter().map(|k| ring.node_for(k).unwrap().to_string()).collect()
    }

    #[test]
    fn nodes_get_a_fair_share() {
        let ring = Ring::new(&["a", "b", "c", "d", "e"], 200);
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for k in keys(50_000) {
            *counts.entry(ring.node_for(&k).unwrap()).or_default() += 1;
        }
        let (max, min) = (counts.values().max().unwrap(), counts.values().min().unwrap());
        assert_eq!(counts.len(), 5);
        assert!((*max as f64) / (*min as f64) < 1.4, "доли узлов неровные: {counts:?}");
    }

    #[test]
    fn a_new_node_takes_only_its_share_from_everyone() {
        let ks = keys(30_000);
        let before = Ring::new(&["a", "b", "c", "d", "e"], 200);
        let mut after = Ring::new(&["a", "b", "c", "d", "e"], 200);
        after.add("f");
        let (x, y) = (assignment(&before, &ks), assignment(&after, &ks));
        let moved = x.iter().zip(&y).filter(|(a, b)| a != b).count();
        assert!(moved < 30_000 * 24 / 100, "при добавлении шестого узла переехало {moved} из 30 000 ключей; честная доля примерно 1/6");
        assert!(moved > 30_000 * 10 / 100, "новый узел получил слишком мало ключей: {moved}");
        assert!(x.iter().zip(&y).all(|(a, b)| a == b || b == "f"), "ключи должны переезжать только на новый узел, а не между старыми");
    }

    #[test]
    fn removing_a_node_moves_only_its_keys() {
        let ks = keys(30_000);
        let before = Ring::new(&["a", "b", "c", "d", "e"], 200);
        let mut after = Ring::new(&["a", "b", "c", "d", "e"], 200);
        after.remove("c");
        let (x, y) = (assignment(&before, &ks), assignment(&after, &ks));
        assert!(x.iter().zip(&y).all(|(a, b)| a == b || a == "c"), "переехали ключи, которые принадлежали живым узлам");
        assert!(y.iter().all(|n| n != "c"));
    }

    #[test]
    fn the_result_does_not_depend_on_the_order_of_adding() {
        let ks = keys(2_000);
        let a = Ring::new(&["a", "b", "c"], 100);
        let b = Ring::new(&["c", "a", "b"], 100);
        assert_eq!(assignment(&a, &ks), assignment(&b, &ks), "кольцо должно быть одним и тем же на всех узлах кластера");
    }

    #[test]
    fn an_empty_ring_owns_nothing_and_one_node_owns_everything() {
        assert!(Ring::new(&[], 100).node_for("x").is_none());
        let one = Ring::new(&["solo"], 10);
        assert!(keys(100).iter().all(|k| one.node_for(k) == Some("solo")));
    }
}
