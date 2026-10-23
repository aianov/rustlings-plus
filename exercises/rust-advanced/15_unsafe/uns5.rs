// Редактор документа даёт две вспомогательные функции: изменяемо взять два элемента сразу и взять два окна
// в буфере. Обе безопасные с виду (`pub fn`, без слова `unsafe` в сигнатуре) и обе внутри собирают пару `&mut`
// из одного указателя. Проверяются только границы. Тесты на удачных входах зелёные, но вызови функции с
// совпадающими или пересекающимися аргументами, и двое «уникальных» владельцев смотрят на одни и те же байты.
// TODO: Почини обе функции. Опасные входы должны давать `None`, остальные работают как раньше.
// Пустое окно никому не мешает: оно ничего не занимает.
use std::ops::Range;
use std::slice;

pub fn pair_mut<T>(values: &mut [T], i: usize, j: usize) -> Option<(&mut T, &mut T)> {
    if i >= values.len() || j >= values.len() {
        return None;
    }
    let base = values.as_mut_ptr();
    unsafe { Some((&mut *base.add(i), &mut *base.add(j))) }
}

pub fn two_windows<T>(values: &mut [T], a: Range<usize>, b: Range<usize>) -> Option<(&mut [T], &mut [T])> {
    let len = values.len();
    if a.start > a.end || b.start > b.end || a.end > len || b.end > len {
        return None;
    }
    let base = values.as_mut_ptr();
    unsafe { Some((slice::from_raw_parts_mut(base.add(a.start), a.len()), slice::from_raw_parts_mut(base.add(b.start), b.len()))) }
}

fn main() {
    let mut v = [1, 2, 3];
    println!("{}", pair_mut(&mut v, 0, 0).is_none());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_different_elements_are_editable_together() {
        let mut v = [1, 2, 3, 4];
        let (a, b) = pair_mut(&mut v, 1, 3).unwrap();
        *a += 10;
        *b += 20;
        assert_eq!(v, [1, 12, 3, 24]);
    }

    #[test]
    fn the_order_of_arguments_is_the_order_of_results() {
        let mut v = ["a".to_string(), "b".to_string(), "c".to_string()];
        let (x, y) = pair_mut(&mut v, 2, 0).unwrap();
        assert_eq!((x.as_str(), y.as_str()), ("c", "a"));
    }

    #[test]
    fn the_same_element_twice_is_refused() {
        let mut v = [1, 2, 3];
        assert!(pair_mut(&mut v, 1, 1).is_none(), "две изменяемые ссылки на один элемент выдавать нельзя");
        assert!(pair_mut(&mut v, 0, 3).is_none());
        assert!(pair_mut(&mut v, 9, 0).is_none());
    }

    #[test]
    fn disjoint_windows_are_independent() {
        let mut v = [0_u8; 8];
        let (a, b) = two_windows(&mut v, 0..3, 5..8).unwrap();
        a.fill(1);
        b.fill(2);
        assert_eq!(v, [1, 1, 1, 0, 0, 2, 2, 2]);
    }

    #[test]
    fn adjacent_windows_and_reversed_order_are_fine() {
        let mut v = [0_u8; 6];
        let (a, b) = two_windows(&mut v, 3..6, 0..3).unwrap();
        assert_eq!((a.len(), b.len()), (3, 3));
        a.fill(7);
        b.fill(9);
        assert_eq!(v, [9, 9, 9, 7, 7, 7]);
    }

    #[test]
    fn overlapping_windows_are_refused() {
        let mut v = [0_u8; 8];
        assert!(two_windows(&mut v, 0..4, 3..6).is_none(), "окна делят элемент 3");
        assert!(two_windows(&mut v, 3..6, 0..4).is_none(), "то же в обратном порядке");
        assert!(two_windows(&mut v, 2..6, 3..5).is_none(), "одно окно внутри другого");
        assert!(two_windows(&mut v, 1..4, 1..4).is_none(), "одно и то же окно дважды");
    }

    #[test]
    fn empty_windows_never_conflict() {
        let mut v = [0_u8; 5];
        let (a, b) = two_windows(&mut v, 2..2, 0..5).unwrap();
        assert_eq!((a.len(), b.len()), (0, 5));
        let (a, b) = two_windows(&mut v, 0..5, 4..4).unwrap();
        assert_eq!((a.len(), b.len()), (5, 0));
    }

    #[test]
    fn bad_ranges_are_refused() {
        let mut v = [0_u8; 4];
        assert!(two_windows(&mut v, 0..5, 1..2).is_none());
        #[allow(clippy::reversed_empty_ranges)]
        let backwards = 3..1;
        assert!(two_windows(&mut v, backwards, 0..1).is_none());
    }
}
