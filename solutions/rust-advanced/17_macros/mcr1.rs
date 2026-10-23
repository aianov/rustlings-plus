#![allow(unused_macros)]

macro_rules! limit {
    ($name:ident : $ty:ty = $value:expr) => {
        pub const $name: $ty = $value;
    };
}

macro_rules! square {
    ($e:expr) => {{
        let value = $e;
        value * value
    }};
}

macro_rules! name_of {
    ($name:ident) => {
        stringify!($name)
    };
}

macro_rules! default_of {
    ($t:ty) => {
        <$t as Default>::default()
    };
}

limit!(MAX_BIO: usize = 2 * 80);
limit!(RETRY: Option<u8> = Some(3));

fn main() {
    println!("{} {}", MAX_BIO, square!(4));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn limit_declares_a_constant_of_the_given_type() {
        assert_eq!(MAX_BIO, 160);
        assert_eq!(RETRY, Some(3));
        const DOUBLE: usize = MAX_BIO * 2;
        assert_eq!(DOUBLE, 320, "limit! должен давать настоящую const, которую можно использовать в const-выражениях");
    }

    #[test]
    fn square_keeps_the_structure_of_the_expression() {
        assert_eq!(square!(1 + 2), 9, "выражение целиком, а не его кусками: (1 + 2) * (1 + 2)");
        assert_eq!(square!(-3), 9);
        assert_eq!(square!(2.5_f64), 6.25);
    }

    #[test]
    fn square_evaluates_its_argument_once() {
        let calls = Cell::new(0);
        let next = || {
            calls.set(calls.get() + 1);
            calls.get() + 1
        };
        assert_eq!(square!(next()), 4);
        assert_eq!(calls.get(), 1, "аргумент с побочным эффектом вычислен {} раз вместо одного", calls.get());
    }

    #[test]
    fn name_of_returns_the_spelling() {
        let user_id = 5;
        assert_eq!(name_of!(user_id), "user_id");
        assert_eq!(name_of!(String), "String");
        let _ = user_id;
    }

    #[test]
    fn default_of_works_for_any_type() {
        assert!(default_of!(Vec<u8>).is_empty());
        assert_eq!(default_of!(String), "");
        assert_eq!(default_of!((u8, bool)), (0, false));
        assert_eq!(default_of!(Option<i32>), None);
    }
}
