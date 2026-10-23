pub fn numbered(lines: &[&str]) -> Vec<String> {
    lines.iter().enumerate().map(|(i, l)| format!("{}. {l}", i + 1)).collect()
}

pub fn deltas(xs: &[i32]) -> Vec<i32> {
    xs.iter().zip(xs.iter().skip(1)).map(|(a, b)| b - a).collect()
}

pub fn page<T: Clone>(items: &[T], number: usize, size: usize) -> Vec<T> {
    let Some(before) = number.checked_sub(1).and_then(|n| n.checked_mul(size)) else {
        return Vec::new();
    };
    items.iter().skip(before).take(size).cloned().collect()
}

pub fn every_nth_from_end(xs: &[u32], step: usize) -> Vec<u32> {
    if step == 0 {
        return Vec::new();
    }
    xs.iter().rev().step_by(step).copied().collect()
}

pub fn with_header(header: &str, rows: &[&str]) -> Vec<String> {
    std::iter::once(header).chain(rows.iter().copied()).map(String::from).collect()
}

fn main() {
    println!("{:?}", numbered(&["a", "b"]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_starts_at_one() {
        assert_eq!(numbered(&["a", "b"]), vec!["1. a", "2. b"]);
        assert!(numbered(&[]).is_empty());
    }

    #[test]
    fn neighbour_differences() {
        assert_eq!(deltas(&[1, 4, 9, 16]), vec![3, 5, 7]);
        assert!(deltas(&[5]).is_empty(), "из одного элемента разностей нет");
        assert!(deltas(&[]).is_empty());
    }

    #[test]
    fn pages_from_one() {
        let items: Vec<u32> = (1..=7).collect();
        assert_eq!(page(&items, 1, 3), vec![1, 2, 3]);
        assert_eq!(page(&items, 3, 3), vec![7], "последняя страница неполная");
        assert!(page(&items, 4, 3).is_empty(), "за концом пусто");
    }

    #[test]
    fn page_edge_cases_do_not_panic() {
        let items = [1, 2, 3];
        assert!(page(&items, 0, 2).is_empty(), "страницы с нуля нет");
        assert!(page(&items, 1, 0).is_empty(), "размер ноль это пустая страница");
        assert!(page(&items, usize::MAX, 10).is_empty(), "огромный номер не должен переполнять умножение");
    }

    #[test]
    fn every_nth_counts_from_the_end() {
        assert_eq!(every_nth_from_end(&[1, 2, 3, 4, 5, 6, 7], 3), vec![7, 4, 1]);
        assert_eq!(every_nth_from_end(&[1, 2, 3], 1), vec![3, 2, 1]);
        assert!(every_nth_from_end(&[1, 2, 3], 0).is_empty(), "шаг ноль: пусто, а не паника");
    }

    #[test]
    fn header_goes_first() {
        assert_eq!(with_header("id", &["1", "2"]), vec!["id", "1", "2"]);
        assert_eq!(with_header("id", &[]), vec!["id"]);
    }
}
