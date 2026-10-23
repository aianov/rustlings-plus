use std::fmt::Display;
use std::iter::Sum;

pub fn join_all<I>(items: I, sep: &str) -> String
where
    I: IntoIterator,
    I::Item: Display,
{
    let mut out = String::new();
    for (i, x) in items.into_iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        out.push_str(&x.to_string());
    }
    out
}

pub fn largest<I>(items: I) -> Option<I::Item>
where
    I: Iterator,
    I::Item: PartialOrd,
{
    items.fold(None, |best, x| match best {
        Some(b) if b >= x => Some(b),
        _ => Some(x),
    })
}

pub fn total<I>(items: I) -> I::Item
where
    I: Iterator,
    I::Item: Sum<I::Item>,
{
    items.sum()
}

fn main() {
    println!("{}", join_all(vec![1, 2, 3], "-"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_accepts_collections_and_iterators() {
        assert_eq!(join_all(vec![1, 2, 3], ", "), "1, 2, 3");
        assert_eq!(join_all(["a", "b"], "+"), "a+b");
        assert_eq!(join_all((1..4).map(|n| n * n), " "), "1 4 9");
        assert_eq!(join_all(Vec::<u8>::new(), ","), "");
    }

    #[test]
    fn largest_works_for_floats_too() {
        assert_eq!(largest([3, 9, 4].into_iter()), Some(9));
        assert_eq!(largest([1.5, 0.5, 2.5].into_iter()), Some(2.5), "f64 не Ord, но PartialOrd: граница должна быть мягче");
        assert_eq!(largest(["pear", "apple"].into_iter()), Some("pear"));
        assert_eq!(largest(std::iter::empty::<u8>()), None);
    }

    #[test]
    fn total_sums_any_numbers() {
        assert_eq!(total([1, 2, 3].into_iter()), 6);
        assert_eq!(total([0.5, 0.25].into_iter()), 0.75);
        assert_eq!(total((1..=4u64).map(|n| n * 10)), 100);
        assert_eq!(total([1u8, 2].iter().copied()), 3, "ссылки на числа сначала превращают в числа");
    }
}
