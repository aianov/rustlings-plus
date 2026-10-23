#[derive(Default)]
pub struct SortedIds {
    ids: Vec<u64>,
}

impl SortedIds {
    pub fn as_slice(&self) -> &[u64] {
        &self.ids
    }

    pub fn insert(&mut self, id: u64) -> bool {
        match self.ids.binary_search(&id) {
            Ok(_) => false,
            Err(pos) => {
                self.ids.insert(pos, id);
                true
            }
        }
    }

    pub fn contains(&self, id: u64) -> bool {
        self.ids.binary_search(&id).is_ok()
    }

    pub fn count_in(&self, lo: u64, hi: u64) -> usize {
        let start = self.ids.partition_point(|x| *x < lo);
        let end = self.ids.partition_point(|x| *x < hi);
        end.saturating_sub(start)
    }

    pub fn first_at_least(&self, id: u64) -> Option<u64> {
        let pos = self.ids.partition_point(|x| *x < id);
        self.ids.get(pos).copied()
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
