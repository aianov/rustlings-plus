// Подписчики автора лежат в отсортированном `Vec<u64>`: на тысячи элементов это компактнее и быстрее
// дерева, а вставки редкие. Вся логика держится на двух вещах: `binary_search` отвечает «есть ли и где место»,
// а `partition_point` считает, сколько элементов подходит под монотонное условие.
// TODO: Допиши `insert`, `contains`, `count_in` (id в промежутке от `lo` включительно до `hi` не включая) и
// `first_at_least`. Вектор всегда отсортирован и без повторов.
#[derive(Default)]
pub struct SortedIds {
    ids: Vec<u64>,
}

impl SortedIds {
    pub fn as_slice(&self) -> &[u64] {
        &self.ids
    }

    pub fn insert(&mut self, id: u64) -> bool {
        todo!("вставить на своё место; false, если id уже есть")
    }

    pub fn contains(&self, id: u64) -> bool {
        todo!("двоичный поиск")
    }

    pub fn count_in(&self, lo: u64, hi: u64) -> usize {
        todo!("сколько id в [lo, hi)")
    }

    pub fn first_at_least(&self, id: u64) -> Option<u64> {
        todo!("наименьший id, не меньший заданного")
    }
}

fn main() {
    let mut s = SortedIds::default();
    s.insert(5);
    println!("{:?}", s.as_slice());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(ids: &[u64]) -> SortedIds {
        let mut s = SortedIds::default();
        for id in ids {
            s.insert(*id);
        }
        s
    }

    #[test]
    fn insertion_keeps_order_and_rejects_duplicates() {
        let mut s = filled(&[50, 10, 30, 20, 40]);
        assert_eq!(s.as_slice(), &[10, 20, 30, 40, 50]);
        assert!(!s.insert(30), "30 уже есть");
        assert!(s.insert(5));
        assert!(s.insert(99));
        assert_eq!(s.as_slice(), &[5, 10, 20, 30, 40, 50, 99]);
    }

    #[test]
    fn contains_finds_present_and_misses_absent() {
        let s = filled(&[10, 20, 30]);
        assert!(s.contains(10) && s.contains(20) && s.contains(30));
        assert!(!s.contains(5) && !s.contains(25) && !s.contains(31));
        assert!(!SortedIds::default().contains(1));
    }

    #[test]
    fn counting_a_range_is_half_open() {
        let s = filled(&[10, 20, 30, 40, 50]);
        assert_eq!(s.count_in(20, 40), 2, "20 и 30: сорок не входит");
        assert_eq!(s.count_in(0, 1000), 5);
        assert_eq!(s.count_in(21, 29), 0);
        assert_eq!(s.count_in(40, 20), 0, "перевёрнутый промежуток это ноль, а не паника");
        assert_eq!(SortedIds::default().count_in(0, 10), 0);
    }

    #[test]
    fn first_at_least_uses_the_next_slot() {
        let s = filled(&[10, 20, 30]);
        assert_eq!(s.first_at_least(20), Some(20));
        assert_eq!(s.first_at_least(21), Some(30));
        assert_eq!(s.first_at_least(0), Some(10));
        assert_eq!(s.first_at_least(31), None);
    }
}
