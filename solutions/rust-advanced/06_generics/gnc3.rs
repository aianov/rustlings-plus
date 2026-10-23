pub fn sum_csv(line: &str) -> i64 {
    let nums = line.split(',').map(|s| s.trim().parse::<i64>().unwrap()).collect::<Vec<_>>();
    nums.iter().sum()
}

pub fn longest_word_len(text: &str) -> usize {
    let lens: Vec<usize> = text.split_whitespace().map(|w| w.len()).collect();
    lens.iter().copied().max().unwrap_or(0)
}

pub fn average(xs: &[f64]) -> f64 {
    let total: f64 = xs.iter().sum();
    if xs.is_empty() { 0.0 } else { total / xs.len() as f64 }
}

fn main() {
    println!("{} {} {}", sum_csv("1, 2, 3"), longest_word_len("a bb ccc"), average(&[1.0, 2.0]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_sum_does_not_overflow_i32() {
        assert_eq!(sum_csv("1, 2, 3"), 6);
        assert_eq!(sum_csv("3000000000, 3000000000"), 6_000_000_000, "числа не помещаются в i32: разбирать надо как i64");
        assert_eq!(sum_csv("-5,5"), 0);
    }

    #[test]
    fn longest_word() {
        assert_eq!(longest_word_len("a bb ccc dd"), 3);
        assert_eq!(longest_word_len(""), 0);
    }

    #[test]
    fn averages() {
        assert_eq!(average(&[1.0, 2.0, 6.0]), 3.0);
        assert_eq!(average(&[]), 0.0, "среднее пустого набора это 0");
        assert!((average(&[0.1, 0.2]) - 0.15).abs() < 1e-9);
    }
}
