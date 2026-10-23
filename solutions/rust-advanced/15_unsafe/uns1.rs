use std::ptr;

pub fn first_and_last(values: &[i32]) -> Option<(i32, i32)> {
    if values.is_empty() {
        return None;
    }
    let first = values.as_ptr();
    // SAFETY: срез непуст, поэтому первый и последний элементы существуют и лежат внутри одного выделения.
    unsafe { Some((*first, *first.add(values.len() - 1))) }
}

pub fn double_all(values: &mut [i32]) {
    let base = values.as_mut_ptr();
    for index in 0..values.len() {
        // SAFETY: index меньше длины среза, указатель получен из этого же среза и свободен от других ссылок.
        unsafe { *base.add(index) *= 2 };
    }
}

pub fn swap_raw(a: &mut i32, b: &mut i32) {
    let (pa, pb): (*mut i32, *mut i32) = (a, b);
    // SAFETY: оба указателя получены из живых &mut, значит валидны и выровнены; &mut не пересекаются.
    unsafe { ptr::swap(pa, pb) };
}

pub fn distance(values: &[u32], from: usize, to: usize) -> isize {
    assert!(from < values.len() && to < values.len(), "индексы должны быть внутри среза");
    let base = values.as_ptr();
    // SAFETY: оба указателя внутри одного среза, а u32 занимает четыре байта, расстояние кратно размеру.
    unsafe { base.add(to).offset_from(base.add(from)) }
}

fn main() {
    println!("{:?}", first_and_last(&[1, 2, 3]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_and_last_read_the_ends() {
        assert_eq!(first_and_last(&[7]), Some((7, 7)));
        assert_eq!(first_and_last(&[1, 2, 3, 4]), Some((1, 4)));
        assert_eq!(first_and_last(&[]), None, "пустой срез не имеет ни первого, ни последнего");
    }

    #[test]
    fn doubling_goes_through_every_element() {
        let mut v = vec![1, -2, 3, 0];
        double_all(&mut v);
        assert_eq!(v, vec![2, -4, 6, 0]);
        let mut empty: [i32; 0] = [];
        double_all(&mut empty);
    }

    #[test]
    fn doubling_a_subslice_leaves_the_neighbours_alone() {
        let mut v = [1, 1, 1, 1, 1];
        double_all(&mut v[1..4]);
        assert_eq!(v, [1, 2, 2, 2, 1], "соседние элементы трогать нельзя");
    }

    #[test]
    fn swapping_exchanges_the_values() {
        let (mut a, mut b) = (3, 9);
        swap_raw(&mut a, &mut b);
        assert_eq!((a, b), (9, 3));
    }

    #[test]
    fn distance_counts_elements_not_bytes() {
        let v = [10_u32, 20, 30, 40, 50];
        assert_eq!(distance(&v, 1, 4), 3, "считаются элементы, а не байты");
        assert_eq!(distance(&v, 4, 1), -3);
        assert_eq!(distance(&v, 2, 2), 0);
    }

    #[test]
    #[should_panic(expected = "индексы")]
    fn distance_checks_bounds_in_safe_code() {
        distance(&[1, 2, 3], 0, 3);
    }
}
