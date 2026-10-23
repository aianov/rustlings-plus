#![allow(unused_macros)]

macro_rules! index {
    ($($key:expr => [$($post:expr),* $(,)?]),* $(,)?) => {
        std::collections::BTreeMap::<&'static str, Vec<u32>>::from([$(($key, vec![$($post),*])),*])
    };
}

fn main() {
    let index = index! { "rust" => [1, 2] };
    println!("{index:?}");
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    #[test]
    fn builds_the_table() {
        let index = index! { "rust" => [1, 2], "async" => [3], "empty" => [] };
        assert_eq!(index.len(), 3);
        assert_eq!(index["rust"], vec![1, 2]);
        assert_eq!(index["async"], vec![3]);
        assert!(index["empty"].is_empty());
    }

    #[test]
    fn trailing_commas_on_both_levels() {
        let index = index! { "a" => [1, 2,], "b" => [3,], };
        assert_eq!(index["a"], vec![1, 2]);
        assert_eq!(index["b"], vec![3]);
    }

    #[test]
    fn empty_index_is_fine() {
        let index = index! {};
        assert!(index.is_empty());
    }

    #[test]
    fn expressions_are_evaluated_once_in_order() {
        let counter = Cell::new(0);
        let next = || {
            counter.set(counter.get() + 1);
            counter.get()
        };
        let index = index! { "x" => [next(), next()], "y" => [next() * 10] };
        assert_eq!(index["x"], vec![1, 2]);
        assert_eq!(index["y"], vec![30]);
        assert_eq!(counter.get(), 3, "каждое выражение вычисляется один раз");
    }

    #[test]
    fn a_repeated_key_keeps_the_last_list() {
        let index = index! { "k" => [1], "k" => [2] };
        assert_eq!(index.len(), 1);
        assert_eq!(index["k"], vec![2]);
    }
}
