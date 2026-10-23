// Адаптер `.pairs()` режет поток на пары `(a, Some(b))`, а если элементов нечётное число, последний
// выходит как `(a, None)`. Написан он быстро и с двумя дефектами. Первый: нечётный хвост пропадает
// без следа. Второй хитрее: после конца итератор снова опрашивает источник, а не каждый источник
// обязан после `None` продолжать выдавать `None` (в тесте источник «оживает»). Правильный адаптер
// после первого `None` из источника больше его не трогает: у стандартных итераторов для этого есть
// `fuse`, а в своём адаптере то же самое делается флагом.
// TODO: Исправь `Pairs`. Типы и название метода менять не нужно.

pub struct Pairs<I: Iterator> {
    inner: I,
}

impl<I: Iterator> Iterator for Pairs<I> {
    type Item = (I::Item, Option<I::Item>);

    fn next(&mut self) -> Option<Self::Item> {
        let a = self.inner.next()?;
        let b = self.inner.next()?;
        Some((a, Some(b)))
    }
}

pub trait PairsExt: Iterator + Sized {
    fn pairs(self) -> Pairs<Self> {
        Pairs { inner: self }
    }
}

impl<I: Iterator> PairsExt for I {}

fn main() {
    let pairs: Vec<_> = (1..=5).pairs().collect();
    println!("{pairs:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Flaky(VecDeque<Option<u32>>);

    impl Iterator for Flaky {
        type Item = u32;

        fn next(&mut self) -> Option<u32> {
            self.0.pop_front().flatten()
        }
    }

    #[test]
    fn odd_tail_is_not_lost() {
        let got: Vec<_> = (1..=5).pairs().collect();
        assert_eq!(got, vec![(1, Some(2)), (3, Some(4)), (5, None)], "пятый элемент остался без пары, но пропадать не должен");
    }

    #[test]
    fn even_and_empty_inputs() {
        assert_eq!((1..=4).pairs().collect::<Vec<_>>(), vec![(1, Some(2)), (3, Some(4))]);
        assert_eq!(std::iter::empty::<u8>().pairs().count(), 0);
        assert_eq!(std::iter::once(7).pairs().collect::<Vec<_>>(), vec![(7, None)]);
    }

    #[test]
    fn finished_means_finished() {
        let source = Flaky(VecDeque::from([Some(1), Some(2), Some(3), None, Some(4), Some(5)]));
        let mut pairs = source.pairs();
        assert_eq!(pairs.next(), Some((1, Some(2))));
        assert_eq!(pairs.next(), Some((3, None)));
        assert_eq!(pairs.next(), None, "источник кончился, значит, всё");
        assert_eq!(pairs.next(), None, "и снова спрашивать его нельзя: он ожил бы и отдал четвёрку");
        assert_eq!(pairs.next(), None);
    }

    #[test]
    fn exhausted_before_the_first_element() {
        let mut pairs = Flaky(VecDeque::from([None, Some(9)])).pairs();
        assert_eq!(pairs.next(), None);
        assert_eq!(pairs.next(), None, "пустой источник, ожививший себя позже, адаптер уже не интересует");
    }
}
