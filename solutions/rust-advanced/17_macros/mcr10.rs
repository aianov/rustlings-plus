#![allow(unused_macros)]

macro_rules! count {
    () => {
        0usize
    };
    ($head:expr $(, $tail:expr)* $(,)?) => {
        1usize + count!($($tail),*)
    };
}

macro_rules! max_of {
    ($first:expr $(, $rest:expr)* $(,)?) => {
        max_of!(@acc $first; $($rest),*)
    };
    (@acc $acc:expr;) => {
        $acc
    };
    (@acc $acc:expr; $next:expr $(, $rest:expr)*) => {
        {
            let current = $acc;
            let candidate = $next;
            max_of!(@acc if current >= candidate { current } else { candidate }; $($rest),*)
        }
    };
}

fn main() {
    println!("{} {}", count!(1, 2, 3), max_of!(4, 9, 2));
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    #[test]
    fn count_counts_without_evaluating() {
        assert_eq!(count!(), 0);
        assert_eq!(count!(1), 1);
        assert_eq!(count!(1, 2, 3,), 3);
        assert_eq!(count!(panic!("не вычисляется"), unreachable!()), 2);
    }

    #[test]
    fn count_is_a_constant() {
        const N: usize = count!("a", "b", "c", "d");
        assert_eq!(N, 4);
        let array = [0u8; count!(1, 2, 3, 4, 5)];
        assert_eq!(array.len(), 5);
    }

    #[test]
    fn count_handles_twenty_arguments() {
        assert_eq!(count!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20), 20);
    }

    #[test]
    fn max_of_picks_the_biggest() {
        assert_eq!(max_of!(7), 7);
        assert_eq!(max_of!(3, 9, 2), 9);
        assert_eq!(max_of!(9, 3, 2,), 9);
        assert_eq!(max_of!(1.5, 0.5), 1.5);
        assert_eq!(max_of!("pear", "apple", "plum"), "plum");
    }

    #[test]
    fn max_of_evaluates_each_argument_once() {
        let calls = Cell::new(0);
        let next = || {
            calls.set(calls.get() + 1);
            calls.get() * 10
        };
        assert_eq!(max_of!(next(), next(), next()), 30);
        assert_eq!(calls.get(), 3, "каждый аргумент вычислен ровно один раз");
    }

    #[test]
    fn max_of_keeps_the_expression_structure() {
        assert_eq!(max_of!(1 + 1, 2 * 2, 10 - 8), 4);
    }
}
