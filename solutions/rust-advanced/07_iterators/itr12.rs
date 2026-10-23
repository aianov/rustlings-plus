pub fn running_balance(start: i64, deltas: &[i64]) -> Vec<i64> {
    deltas
        .iter()
        .scan(start, |balance, d| {
            *balance += d;
            Some(*balance)
        })
        .collect()
}

pub fn balances_until_overdraft(start: i64, deltas: &[i64]) -> Vec<i64> {
    deltas
        .iter()
        .scan(start, |balance, d| {
            *balance += d;
            (*balance >= 0).then_some(*balance)
        })
        .collect()
}

pub fn longest_word<'a>(words: &[&'a str]) -> Option<&'a str> {
    words.iter().copied().reduce(|best, w| if w.len() > best.len() { w } else { best })
}

pub fn min_max(xs: &[i32]) -> Option<(i32, i32)> {
    xs.iter().copied().fold(None, |acc, x| match acc {
        None => Some((x, x)),
        Some((lo, hi)) => Some((lo.min(x), hi.max(x))),
    })
}

pub fn longest_streak(flags: &[bool]) -> usize {
    flags
        .iter()
        .fold((0, 0), |(current, best), &flag| {
            let current = if flag { current + 1 } else { 0 };
            (current, best.max(current))
        })
        .1
}

fn main() {
    println!("{:?}", running_balance(10, &[5, -3]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balances_accumulate() {
        assert_eq!(running_balance(100, &[-30, 50, -20]), vec![70, 120, 100]);
        assert!(running_balance(5, &[]).is_empty());
    }

    #[test]
    fn overdraft_stops_the_stream() {
        assert_eq!(balances_until_overdraft(10, &[-4, -5, -3, 20]), vec![6, 1], "после ухода в минус ничего не выдаётся, даже если баланс снова вырос");
        assert_eq!(balances_until_overdraft(0, &[5, -5, 1]), vec![5, 0, 1], "ровно ноль ещё не овердрафт");
    }

    #[test]
    fn longest_word_prefers_the_first() {
        assert_eq!(longest_word(&["aa", "bbb", "ccc", "d"]), Some("bbb"));
        assert_eq!(longest_word(&[]), None);
    }

    #[test]
    fn min_and_max_in_one_pass() {
        assert_eq!(min_max(&[3, -1, 7, 2]), Some((-1, 7)));
        assert_eq!(min_max(&[4]), Some((4, 4)));
        assert_eq!(min_max(&[]), None);
    }

    #[test]
    fn streaks() {
        assert_eq!(longest_streak(&[true, true, false, true, true, true, false]), 3);
        assert_eq!(longest_streak(&[false, false]), 0);
        assert_eq!(longest_streak(&[]), 0);
        assert_eq!(longest_streak(&[true, true]), 2, "серия, дошедшая до конца, тоже считается");
    }
}
