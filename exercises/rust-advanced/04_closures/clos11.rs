// Заготовки для библиотеки функциональных мелочей. `compose` склеивает две функции в одну, а `memoize`
// оборачивает дорогую функцию так, чтобы для уже виденного аргумента она не пересчитывалась: результаты
// запоминаются в таблице, которая живёт внутри возвращённого замыкания. Тест подсовывает функцию-счётчик
// и считает, сколько раз её на самом деле вызвали.
// TODO: Реализуй обе функции. Аргументы `compose` разных типов, результат `memoize` должен уметь
// менять своё внутреннее состояние.

pub fn compose<A, B, C>(f: impl Fn(A) -> B, g: impl Fn(B) -> C) -> impl Fn(A) -> C {
    todo!("сначала f, потом g")
}

pub fn memoize(f: impl Fn(u64) -> u64) -> impl FnMut(u64) -> u64 {
    todo!("запоминать результаты в HashMap внутри замыкания")
}

fn main() {
    let f = compose(|x: i32| x + 1, |y| y.to_string());
    println!("{}", f(1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn compose_changes_types_along_the_way() {
        let len_of_double = compose(|n: u32| (n * 2).to_string(), |s: String| s.len());
        assert_eq!(len_of_double(5), 2, "5 -> \"10\" -> 2");
        assert_eq!(len_of_double(500), 4);
    }

    #[test]
    fn compose_order_matters() {
        let a = compose(|x: i32| x + 1, |x: i32| x * 10);
        let b = compose(|x: i32| x * 10, |x: i32| x + 1);
        assert_eq!((a(1), b(1)), (20, 11), "f применяется первой");
    }

    #[test]
    fn compose_chains() {
        let f = compose(compose(|x: i32| x + 1, |x| x * 2), |x| x - 3);
        assert_eq!(f(4), 7);
    }

    #[test]
    fn memoize_calls_original_once_per_argument() {
        let calls = Cell::new(0);
        let mut fast = memoize(|x| {
            calls.set(calls.get() + 1);
            x * x
        });
        assert_eq!(fast(4), 16);
        assert_eq!(fast(4), 16);
        assert_eq!(fast(5), 25);
        assert_eq!(fast(4), 16);
        assert_eq!(calls.get(), 2, "исходная функция должна считать каждый аргумент только один раз");
    }

    #[test]
    fn memoize_keeps_distinct_results() {
        let mut f = memoize(|x| x + 1);
        let got: Vec<u64> = [1, 2, 1, 3, 2].iter().map(|&x| f(x)).collect();
        assert_eq!(got, vec![2, 3, 2, 4, 3], "кэш не должен путать аргументы");
    }
}
