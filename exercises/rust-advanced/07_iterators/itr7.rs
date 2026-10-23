// Пять небольших функций над последовательностями, и каждую можно написать циклом с индексами, но
// выразительнее адаптерами `enumerate`, `zip`, `chain`, `rev`, `skip`, `take`, `step_by`. Цель не в том,
// чтобы «сделать красиво», а в том, чтобы краевые случаи (пустой ввод, слишком большая страница,
// нулевой шаг) не приводили к панике и сдвигам на единицу. Номера страниц идут с единицы.
// TODO: Реализуй функции. `page` не должен паниковать на любом номере страницы (даже огромном) и размере
// ноль; `every_nth_from_end` с шагом ноль даёт пустой результат.

pub fn numbered(lines: &[&str]) -> Vec<String> {
    todo!("`1. первая`, `2. вторая`, ...")
}

pub fn deltas(xs: &[i32]) -> Vec<i32> {
    todo!("разности соседних: x[i+1] - x[i]")
}

pub fn page<T: Clone>(items: &[T], number: usize, size: usize) -> Vec<T> {
    todo!("страница с единицы")
}

pub fn every_nth_from_end(xs: &[u32], step: usize) -> Vec<u32> {
    todo!("идём с конца: последний, потом каждый step-й")
}

pub fn with_header(header: &str, rows: &[&str]) -> Vec<String> {
    todo!("заголовок, затем строки")
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
