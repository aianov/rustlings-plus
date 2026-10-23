// Не каждому итератору нужна своя структура. `iter::successors` строит последовательность из «предыдущее
// даёт следующее», `iter::from_fn` из замыкания с состоянием, `iter::once` и `iter::repeat_with` закрывают
// мелкие случаи. Пять функций, и в каждой нужна своя заготовка.
// TODO: Реализуй функции без собственных структур и без ручных циклов. Правила в комментариях.

// Степени двойки строго меньше лимита: 1, 2, 4, ... Переполнение u64 это конец ряда.
pub fn powers_of_two_below(limit: u64) -> Vec<u64> {
    todo!()
}

// Ряд Коллатца от n до единицы включительно: чётное делим на два, нечётное превращаем в 3n+1.
// Для нуля ряд пуст (иначе он бесконечен).
pub fn collatz(n: u64) -> Vec<u64> {
    todo!()
}

// `count` номеров билетов вида `T-0007`, начиная с `start`.
pub fn ticket_numbers(start: u32, count: usize) -> Vec<String> {
    todo!()
}

// Элементы, а затем ещё один замыкающий.
pub fn with_sentinel(items: &[u32], sentinel: u32) -> Vec<u32> {
    todo!()
}

// `n` псевдослучайных чисел: состояние x начинается с seed; на каждом шаге x = x * 1103515245 + 12345
// (с переполнением по модулю 2^32), выдаётся (x >> 16) & 0x7fff.
pub fn noise(seed: u32, n: usize) -> Vec<u32> {
    todo!()
}

fn main() {
    println!("{:?}", powers_of_two_below(20));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powers() {
        assert_eq!(powers_of_two_below(20), vec![1, 2, 4, 8, 16]);
        assert_eq!(powers_of_two_below(16), vec![1, 2, 4, 8], "строго меньше лимита");
        assert!(powers_of_two_below(1).is_empty());
        assert_eq!(powers_of_two_below(u64::MAX).len(), 64, "ряд доходит до 2^63 и тихо кончается, а не паникует на переполнении");
    }

    #[test]
    fn collatz_series() {
        assert_eq!(collatz(6), vec![6, 3, 10, 5, 16, 8, 4, 2, 1]);
        assert_eq!(collatz(1), vec![1]);
        assert!(collatz(0).is_empty());
    }

    #[test]
    fn tickets() {
        assert_eq!(ticket_numbers(7, 3), vec!["T-0007", "T-0008", "T-0009"]);
        assert!(ticket_numbers(1, 0).is_empty());
        assert_eq!(ticket_numbers(9999, 2), vec!["T-9999", "T-10000"], "ширина не меньше четырёх цифр");
    }

    #[test]
    fn sentinel_goes_last() {
        assert_eq!(with_sentinel(&[1, 2], 0), vec![1, 2, 0]);
        assert_eq!(with_sentinel(&[], 9), vec![9]);
    }

    #[test]
    fn noise_is_deterministic() {
        assert_eq!(noise(1, 4), vec![16838, 5758, 10113, 17515]);
        assert_eq!(noise(1, 2), noise(1, 4)[..2].to_vec(), "тот же seed даёт тот же ряд");
        assert!(noise(5, 0).is_empty());
    }
}
