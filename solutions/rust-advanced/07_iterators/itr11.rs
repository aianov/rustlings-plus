use std::num::ParseIntError;

pub fn parse_all(items: &[&str]) -> Result<Vec<i32>, ParseIntError> {
    items.iter().map(|s| s.parse()).collect()
}

pub fn all_present(xs: &[Option<u32>]) -> Option<Vec<u32>> {
    xs.iter().copied().collect()
}

pub fn split_even_odd(xs: &[u32]) -> (Vec<u32>, Vec<u32>) {
    xs.iter().copied().partition(|x| x % 2 == 0)
}

pub fn unzip_pairs<'a>(pairs: &[(u32, &'a str)]) -> (Vec<u32>, Vec<&'a str>) {
    pairs.iter().copied().unzip()
}

pub fn good_and_bad(items: &[&str]) -> (Vec<i32>, Vec<String>) {
    let (good, bad): (Vec<_>, Vec<_>) = items.iter().map(|s| s.parse::<i32>().map_err(|e| format!("{s}: {e}"))).partition(|r| r.is_ok());
    (good.into_iter().flatten().collect(), bad.into_iter().filter_map(Result::err).collect())
}

fn main() {
    println!("{:?}", parse_all(&["1", "2"]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_error_wins() {
        assert_eq!(parse_all(&["1", "-2", "3"]), Ok(vec![1, -2, 3]));
        let err = parse_all(&["1", "x", "y"]).expect_err("x не число");
        assert_eq!(err.to_string(), "invalid digit found in string");
        assert_eq!(parse_all(&[]), Ok(vec![]));
    }

    #[test]
    fn options_collect_all_or_nothing() {
        assert_eq!(all_present(&[Some(1), Some(2)]), Some(vec![1, 2]));
        assert_eq!(all_present(&[Some(1), None, Some(3)]), None);
        assert_eq!(all_present(&[]), Some(vec![]), "пустой набор: все на месте");
    }

    #[test]
    fn partition_and_unzip() {
        assert_eq!(split_even_odd(&[1, 2, 3, 4, 6]), (vec![2, 4, 6], vec![1, 3]));
        assert_eq!(unzip_pairs(&[(1, "a"), (2, "b")]), (vec![1, 2], vec!["a", "b"]));
        assert_eq!(unzip_pairs(&[]), (vec![], vec![]));
    }

    #[test]
    fn bad_records_are_separated_not_fatal() {
        let (good, bad) = good_and_bad(&["1", "x", "3", "", "5"]);
        assert_eq!(good, vec![1, 3, 5]);
        assert_eq!(bad, vec!["x: invalid digit found in string", ": cannot parse integer from empty string"]);
    }
}
