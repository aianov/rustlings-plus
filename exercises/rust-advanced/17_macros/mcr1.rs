// Профиль соцсети описывается константами, проверками и мелкими утилитами, и все они похожи, но с разными
// именами и типами. Такое удобно оформлять макросами: макрос получает не значения, а куски кода, и каждому
// куску можно назвать вид: `expr` (выражение), `ident` (имя), `ty` (тип), `literal`, `tt` (один токен
// или группа в скобках). Нужны четыре макроса:
// `limit!(ИМЯ: Тип = выражение)` объявляет `pub const ИМЯ: Тип = выражение;`;
// `square!(выражение)` даёт квадрат значения, а выражение вычисляется один раз (внутри может быть вызов
// с побочным эффектом);
// `name_of!(имя)` возвращает написанное имя строкой `&'static str`;
// `default_of!(Тип)` возвращает значение по умолчанию этого типа.
// TODO: Напиши все четыре макроса. Определения макросов должны стоять выше мест, где их зовут.
#![allow(unused_macros)]

macro_rules! limit {
    () => {};
}

macro_rules! square {
    () => {};
}

macro_rules! name_of {
    () => {};
}

macro_rules! default_of {
    () => {};
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
