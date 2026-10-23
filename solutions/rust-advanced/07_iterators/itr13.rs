use std::cell::Cell;

pub fn total_checked(items: &[&str], visited: &Cell<usize>) -> Result<u32, String> {
    items.iter().try_fold(0u32, |sum, s| {
        visited.set(visited.get() + 1);
        let n = s.trim().parse::<u32>().map_err(|e| format!("{s:?}: {e}"))?;
        sum.checked_add(n).ok_or_else(|| "overflow".to_string())
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
