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

impl Extend<String> for Bag {
    fn extend<T: IntoIterator<Item = String>>(&mut self, iter: T) {
        for word in iter {
            *self.counts.entry(word).or_insert(0) += 1;
        }
    }
}

impl<'a> Extend<&'a str> for Bag {
    fn extend<T: IntoIterator<Item = &'a str>>(&mut self, iter: T) {
        self.extend(iter.into_iter().map(String::from));
    }
}

impl FromIterator<String> for Bag {
    fn from_iter<T: IntoIterator<Item = String>>(iter: T) -> Bag {
        let mut bag = Bag::new();
        bag.extend(iter);
        bag
    }
}

impl<'a> FromIterator<&'a str> for Bag {
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Bag {
        let mut bag = Bag::new();
        bag.extend(iter);
        bag
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
