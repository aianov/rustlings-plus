// Подсчёт суммы строк-чисел сделан через `fold`, и тесты на ошибки красные по двум причинам. Первая:
// после первой же плохой строки цикл продолжает разбирать остальные (счётчик `visited` показывает
// лишнюю работу, а для настоящей сети это лишние запросы). Вторая: сложение переполняется молча, и сумма
// выходит неверной. Условия: на первой ошибке разбора остановиться и вернуть её в виде `"<строка>": <текст>`
// (строка в кавычках через `{:?}`); при переполнении `u32` вернуть `overflow`.
// TODO: Перепиши `total_checked` так, чтобы обход прерывался на первой ошибке, а не просто запоминал её.
use std::cell::Cell;

pub fn total_checked(items: &[&str], visited: &Cell<usize>) -> Result<u32, String> {
    items.iter().fold(Ok(0u32), |acc, s| {
        visited.set(visited.get() + 1);
        let n = s.trim().parse::<u32>().map_err(|e| format!("{s:?}: {e}"));
        match (acc, n) {
            (Ok(sum), Ok(n)) => Ok(sum.wrapping_add(n)),
            (Err(e), _) | (Ok(_), Err(e)) => Err(e),
        }
    })
}

fn main() {
    let visited = Cell::new(0);
    println!("{:?}", total_checked(&["1", "2"], &visited));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_good_input() {
        let visited = Cell::new(0);
        assert_eq!(total_checked(&["1", " 2 ", "40"], &visited), Ok(43));
        assert_eq!(visited.get(), 3);
        assert_eq!(total_checked(&[], &Cell::new(0)), Ok(0));
    }

    #[test]
    fn stops_at_the_first_bad_item() {
        let visited = Cell::new(0);
        let err = total_checked(&["1", "x", "3", "4"], &visited).expect_err("x не число");
        assert!(err.starts_with("\"x\": "), "сообщение начинается со строки в кавычках: {err}");
        assert_eq!(visited.get(), 2, "после ошибки остальные строки не разбираются");
    }

    #[test]
    fn overflow_is_an_error_and_stops_too() {
        let visited = Cell::new(0);
        let big = u32::MAX.to_string();
        let err = total_checked(&[&big, "1", "2", "3"], &visited);
        assert_eq!(err, Err("overflow".to_string()), "сумма не помещается в u32");
        assert_eq!(visited.get(), 2, "после переполнения обход тоже прекращается");
    }

    #[test]
    fn exact_maximum_is_fine() {
        let half = (u32::MAX / 2).to_string();
        assert_eq!(total_checked(&[&half, &half, "1"], &Cell::new(0)), Ok(u32::MAX));
    }
}
