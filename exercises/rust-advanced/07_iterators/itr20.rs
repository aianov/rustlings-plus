// Мешок слов считает, сколько раз встретилось каждое слово. Хочется строить его так же, как `Vec` и
// `HashMap`: `let bag: Bag = words.into_iter().collect()` и `bag.extend(more)`. Для этого нужны трейты
// `FromIterator` (для `collect`) и `Extend` (для `extend`). Принимать надо и `String`, и `&str`, чтобы
// не заставлять вызывающего копировать.
// TODO: Реализуй `FromIterator` и `Extend` для обоих видов элементов. Подсказка: одну реализацию можно
// выразить через другую.
use std::collections::BTreeMap;

#[derive(Default, Debug, PartialEq)]
pub struct Bag {
    counts: BTreeMap<String, usize>,
}

impl Bag {
    pub fn new() -> Bag {
        Bag::default()
    }

    pub fn count(&self, word: &str) -> usize {
        self.counts.get(word).copied().unwrap_or(0)
    }

    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    pub fn distinct(&self) -> usize {
        self.counts.len()
    }
}

fn main() {
    let bag: Bag = "a b a".split(' ').collect();
    println!("{}", bag.count("a"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_from_strs() {
        let bag: Bag = "to be or not to be".split(' ').collect();
        assert_eq!(bag.count("to"), 2);
        assert_eq!(bag.count("not"), 1);
        assert_eq!(bag.count("missing"), 0);
        assert_eq!((bag.total(), bag.distinct()), (6, 4));
    }

    #[test]
    fn collect_from_strings() {
        let bag: Bag = (0..6).map(|n| (n % 3).to_string()).collect();
        assert_eq!(bag.count("0"), 2);
        assert_eq!(bag.distinct(), 3);
    }

    #[test]
    fn extend_adds_to_existing_counts() {
        let mut bag: Bag = ["a", "b"].into_iter().collect();
        bag.extend(["a", "a"]);
        bag.extend(vec!["b".to_string()]);
        assert_eq!(bag.count("a"), 3);
        assert_eq!(bag.count("b"), 2);
    }

    #[test]
    fn empty_input_gives_an_empty_bag() {
        let bag: Bag = std::iter::empty::<&str>().collect();
        assert_eq!(bag, Bag::new());
        assert_eq!(bag.total(), 0);
    }

    #[test]
    fn works_with_the_turbofish() {
        let total = ["x", "y", "x"].into_iter().collect::<Bag>().total();
        assert_eq!(total, 3);
    }
}
