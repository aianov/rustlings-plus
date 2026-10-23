pub fn my_split_at_mut<T>(values: &mut [T], mid: usize) -> (&mut [T], &mut [T]) {
    let len = values.len();
    assert!(mid <= len, "mid ({mid}) больше длины среза ({len})");
    let base = values.as_mut_ptr();
    // SAFETY: mid не больше len, поэтому [0, mid) и [mid, len) лежат внутри среза и не пересекаются;
    // исходный &mut заимствован на время результата, других ссылок на эти элементы нет.
    unsafe { (std::slice::from_raw_parts_mut(base, mid), std::slice::from_raw_parts_mut(base.add(mid), len - mid)) }
}

pub fn first_two_mut<T>(values: &mut [T]) -> Option<(&mut T, &mut T)> {
    if values.len() < 2 {
        return None;
    }
    let (left, right) = my_split_at_mut(values, 1);
    Some((&mut left[0], &mut right[0]))
}

fn main() {
    let mut v = [1, 2, 3, 4];
    let (a, b) = my_split_at_mut(&mut v, 2);
    a[0] = b[1];
    println!("{v:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    struct Token<'a>(&'a Cell<usize>);

    impl Drop for Token<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn halves_are_independent() {
        let mut v = [1, 2, 3, 4, 5];
        let (left, right) = my_split_at_mut(&mut v, 2);
        assert_eq!((left.len(), right.len()), (2, 3));
        left[0] = 10;
        right[2] = 50;
        std::mem::swap(&mut left[1], &mut right[0]);
        assert_eq!(v, [10, 3, 2, 4, 50]);
    }

    #[test]
    fn the_edges_give_an_empty_half() {
        let mut v = [1, 2, 3];
        let (a, b) = my_split_at_mut(&mut v, 0);
        assert_eq!((a.len(), b.len()), (0, 3));
        let (a, b) = my_split_at_mut(&mut v, 3);
        assert_eq!((a.len(), b.len()), (3, 0));
        let mut empty: [u8; 0] = [];
        let (a, b) = my_split_at_mut(&mut empty, 0);
        assert!(a.is_empty() && b.is_empty());
    }

    #[test]
    fn a_mid_beyond_the_end_panics_instead_of_reading_garbage() {
        let mut v = [1, 2, 3];
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = my_split_at_mut(&mut v, 4);
        }));
        assert!(result.is_err(), "mid больше длины должен заканчиваться паникой");
    }

    #[test]
    fn owned_elements_are_neither_leaked_nor_dropped_twice() {
        let drops = Cell::new(0);
        {
            let mut tokens = vec![Token(&drops), Token(&drops), Token(&drops), Token(&drops)];
            let (a, b) = my_split_at_mut(&mut tokens, 1);
            std::mem::swap(&mut a[0], &mut b[2]);
            assert_eq!(drops.get(), 0, "разделение ничего не должно освобождать");
        }
        assert_eq!(drops.get(), 4, "каждый элемент освобождается ровно один раз");
    }

    #[test]
    fn works_with_strings_and_zero_sized_types() {
        let mut names = vec![String::from("анна"), String::from("борис"), String::from("вера")];
        let (a, b) = my_split_at_mut(&mut names, 2);
        a[1].push('!');
        b[0].push('?');
        assert_eq!(names, ["анна", "борис!", "вера?"]);
        let mut units = [(); 5];
        let (a, b) = my_split_at_mut(&mut units, 2);
        assert_eq!((a.len(), b.len()), (2, 3));
    }

    #[test]
    fn two_elements_at_once() {
        let mut v = [1, 2, 3];
        let (a, b) = first_two_mut(&mut v).unwrap();
        std::mem::swap(a, b);
        assert_eq!(v, [2, 1, 3]);
        assert!(first_two_mut(&mut [1]).is_none());
        assert!(first_two_mut::<u8>(&mut []).is_none());
    }
}
