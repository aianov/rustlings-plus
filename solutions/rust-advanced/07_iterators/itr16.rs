pub fn powers_of_two_below(limit: u64) -> Vec<u64> {
    std::iter::successors(Some(1u64), |&x| x.checked_mul(2)).take_while(|&x| x < limit).collect()
}

pub fn collatz(n: u64) -> Vec<u64> {
    if n == 0 {
        return Vec::new();
    }
    std::iter::successors(Some(n).filter(|&n| n > 0), |&x| match x {
        1 => None,
        x if x % 2 == 0 => Some(x / 2),
        x => Some(3 * x + 1),
    })
    .collect()
}

pub fn ticket_numbers(start: u32, count: usize) -> Vec<String> {
    let mut next = start;
    std::iter::from_fn(|| {
        let ticket = format!("T-{next:04}");
        next += 1;
        Some(ticket)
    })
    .take(count)
    .collect()
}

pub fn with_sentinel(items: &[u32], sentinel: u32) -> Vec<u32> {
    items.iter().copied().chain(std::iter::once(sentinel)).collect()
}

pub fn noise(seed: u32, n: usize) -> Vec<u32> {
    let mut x = seed;
    std::iter::repeat_with(|| {
        x = x.wrapping_mul(1103515245).wrapping_add(12345);
        (x >> 16) & 0x7fff
    })
    .take(n)
    .collect()
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
