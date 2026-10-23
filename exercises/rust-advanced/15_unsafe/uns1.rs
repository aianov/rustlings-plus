// Сырой указатель это адрес без гарантий: он может быть пустым, висячим или не выровненным. Создавать и
// сравнивать указатели можно в безопасном коде, а вот прочитать или записать по нему только в блоке `unsafe`.
// Он снимает пять запретов компилятора, но заимствования, типы и владение проверяются как раньше.
// TODO: Допиши четыре функции через сырые указатели (`as_ptr`, `as_mut_ptr`, `add`, `offset_from`).
// Границы проверяй обычным безопасным кодом ДО входа в `unsafe`.
use std::ptr;

pub fn first_and_last(values: &[i32]) -> Option<(i32, i32)> {
    todo!("первый и последний элемент через указатели; пустой срез это None")
}

pub fn double_all(values: &mut [i32]) {
    todo!("удвоить каждый элемент, записывая через указатель")
}

pub fn swap_raw(a: &mut i32, b: &mut i32) {
    todo!("обменять значения, превратив обе ссылки в указатели")
}

pub fn distance(values: &[u32], from: usize, to: usize) -> isize {
    todo!("на сколько элементов указатель на `to` дальше указателя на `from` (может быть отрицательным)")
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
