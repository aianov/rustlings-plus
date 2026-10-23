// Отчёт считает квадраты чисел из ленты, а сам расчёт дорогой, и счётчик `calls` показывает, сколько раз
// он выполнялся. Нужна функция, которая возвращает итератор квадратов и при этом не считает ничего,
// пока кто-то не начнёт брать значения: если читатель возьмёт два числа из тысячи, должно произойти два
// вычисления. Тип результата `impl Iterator<Item = u32> + 'a` значит «какой-то итератор, детали скрыты»,
// подробно он разобран в конце главы.
// TODO: Реализуй `squares_counted`. Каждое вычисление квадрата увеличивает `calls` на единицу.
use std::cell::Cell;

pub fn squares_counted<'a>(input: &'a [u32], calls: &'a Cell<usize>) -> impl Iterator<Item = u32> + 'a {
    todo!("не считать, пока никто не просит");
    std::iter::empty()
}

fn main() {
    let calls = Cell::new(0);
    let first: Vec<u32> = squares_counted(&[1, 2, 3], &calls).take(2).collect();
    println!("{first:?} {}", calls.get());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_computed_until_asked() {
        let calls = Cell::new(0);
        let _squares = squares_counted(&[1, 2, 3], &calls);
        assert_eq!(calls.get(), 0, "итератор создан, но никто не просил значений: считать рано");
    }

    #[test]
    fn only_requested_values_are_computed() {
        let calls = Cell::new(0);
        let big = (1..=1000).collect::<Vec<u32>>();
        let first: Vec<u32> = squares_counted(&big, &calls).take(2).collect();
        assert_eq!(first, vec![1, 4]);
        assert_eq!(calls.get(), 2, "взяли два значения из тысячи: два вычисления");
    }

    #[test]
    fn search_stops_at_the_first_hit() {
        let calls = Cell::new(0);
        let found = squares_counted(&[1, 3, 5, 6, 7, 9], &calls).find(|sq| sq % 2 == 0);
        assert_eq!(found, Some(36));
        assert_eq!(calls.get(), 4, "после четвёртого квадрата ответ найден, остальные не считаются");
    }

    #[test]
    fn full_pass_computes_each_once() {
        let calls = Cell::new(0);
        let total: u32 = squares_counted(&[2, 3, 4], &calls).sum();
        assert_eq!(total, 29);
        assert_eq!(calls.get(), 3);
    }
}
