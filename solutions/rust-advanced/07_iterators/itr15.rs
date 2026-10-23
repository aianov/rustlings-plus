use std::iter::Peekable;

pub fn split_batch(it: &mut impl Iterator<Item = u32>) -> (Vec<u32>, Vec<u32>) {
    let first: Vec<u32> = it.by_ref().take(3).collect();
    let rest: Vec<u32> = it.collect();
    (first, rest)
}

pub fn head_and_tail(text: &str) -> (String, String) {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(3).collect();
    let tail: String = chars.collect();
    (head, tail)
}

pub fn split_small(it: &mut Peekable<impl Iterator<Item = u32>>) -> Vec<u32> {
    std::iter::from_fn(|| it.next_if(|&x| x < 10)).collect()
}

fn main() {
    let mut it = 1..=10;
    println!("{:?}", split_batch(&mut it));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_and_rest() {
        let mut it = 1..=10;
        assert_eq!(split_batch(&mut it), (vec![1, 2, 3], vec![4, 5, 6, 7, 8, 9, 10]));
        let mut short = 1..=2;
        assert_eq!(split_batch(&mut short), (vec![1, 2], vec![]));
    }

    #[test]
    fn batch_leaves_the_caller_iterator_drained_only_as_much_as_needed() {
        let mut it = 1..=10u32;
        let first: Vec<u32> = it.by_ref().take(2).collect();
        let (a, _) = split_batch(&mut it);
        assert_eq!((first, a), (vec![1, 2], vec![3, 4, 5]), "функция продолжает с того места, где остановился вызывающий");
    }

    #[test]
    fn head_and_tail_of_text() {
        assert_eq!(head_and_tail("abcdef"), ("abc".to_string(), "def".to_string()));
        assert_eq!(head_and_tail("ab"), ("ab".to_string(), String::new()));
        assert_eq!(head_and_tail("яблоко"), ("ябл".to_string(), "око".to_string()), "считаются символы, а не байты");
    }

    #[test]
    fn small_prefix_keeps_the_first_big_element() {
        let mut it = [1u32, 2, 30, 4, 50].into_iter().peekable();
        assert_eq!(split_small(&mut it), vec![1, 2]);
        assert_eq!(it.next(), Some(30), "первый большой элемент не должен пропасть");
        assert_eq!(it.collect::<Vec<_>>(), vec![4, 50]);
    }

    #[test]
    fn small_prefix_edge_cases() {
        let mut empty = Vec::<u32>::new().into_iter().peekable();
        assert!(split_small(&mut empty).is_empty());
        let mut all_small = [1u32, 2, 3].into_iter().peekable();
        assert_eq!(split_small(&mut all_small), vec![1, 2, 3]);
        assert_eq!(all_small.next(), None);
    }
}
