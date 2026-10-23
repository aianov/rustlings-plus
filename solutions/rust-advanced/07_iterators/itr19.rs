pub struct Span {
    lo: u32,
    hi: u32,
}

impl Span {
    pub fn new(lo: u32, hi: u32) -> Span {
        Span { lo, hi }
    }
}

impl Iterator for Span {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.lo >= self.hi {
            return None;
        }
        let value = self.lo;
        self.lo += 1;
        Some(value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.hi.saturating_sub(self.lo) as usize;
        (left, Some(left))
    }
}

impl ExactSizeIterator for Span {}

impl DoubleEndedIterator for Span {
    fn next_back(&mut self) -> Option<u32> {
        if self.lo >= self.hi {
            return None;
        }
        self.hi -= 1;
        Some(self.hi)
    }
}

fn main() {
    println!("{:?}", Span::new(1, 4).collect::<Vec<_>>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_is_exact_and_shrinks() {
        let mut s = Span::new(3, 8);
        assert_eq!(s.size_hint(), (5, Some(5)));
        s.next();
        s.next();
        assert_eq!(s.size_hint(), (3, Some(3)), "после двух next осталось три");
        assert_eq!(Span::new(5, 5).size_hint(), (0, Some(0)));
        assert_eq!(Span::new(9, 2).size_hint(), (0, Some(0)), "перевёрнутый диапазон пуст, а не переполняется");
    }

    #[test]
    fn len_works() {
        assert_eq!(Span::new(0, 10).len(), 10);
        let mut s = Span::new(0, 10);
        s.next_back();
        assert_eq!(s.len(), 9);
    }

    #[test]
    fn reversing() {
        assert_eq!(Span::new(1, 5).rev().collect::<Vec<_>>(), vec![4, 3, 2, 1]);
        assert_eq!(Span::new(1, 1).rev().count(), 0);
    }

    #[test]
    fn both_ends_meet_without_overlap() {
        let mut s = Span::new(0, 4);
        assert_eq!(s.next(), Some(0));
        assert_eq!(s.next_back(), Some(3));
        assert_eq!(s.next(), Some(1));
        assert_eq!(s.next_back(), Some(2));
        assert_eq!(s.next(), None, "концы встретились: больше ничего нет");
        assert_eq!(s.next_back(), None);
    }

    #[test]
    fn adapters_that_need_both_traits() {
        let pairs: Vec<(u32, u32)> = Span::new(0, 3).zip(Span::new(10, 13)).rev().collect();
        assert_eq!(pairs, vec![(2, 12), (1, 11), (0, 10)]);
        let numbered: Vec<(usize, u32)> = Span::new(5, 8).enumerate().rev().collect();
        assert_eq!(numbered, vec![(2, 7), (1, 6), (0, 5)]);
        assert_eq!(Span::new(0, 100).rev().take(2).collect::<Vec<_>>(), vec![99, 98]);
    }
}
