// Команды чата приходят как список слов: `["ann", "5", "true"]`. Тип, который умеет собираться из слов,
// описан трейтом `FromParts`: для одиночных типов (`u32`, `String`...) он уже реализован, а для кортежей
// разной длины реализации пишет макрос, потому что вручную `(A,)`, `(A, B)`, `(A, B, C)`, `(A, B, C, D)`
// получатся четыре почти одинаковых куска. Кортеж собирается, только если слов ровно столько, сколько
// полей, и каждое слово разобралось.
// Макрос написан, но не собирается, как только в списке появляется кортеж из одного элемента.
// TODO: Почини макрос и добавь реализации для кортежей до четырёх элементов.
#![allow(unused_macros)]

use std::str::FromStr;

pub trait FromParts: Sized {
    fn from_parts(parts: &[&str]) -> Option<Self>;
}

macro_rules! scalar_from_parts {
    ($($t:ty),*) => {
        $(
            impl FromParts for $t {
                fn from_parts(parts: &[&str]) -> Option<Self> {
                    match parts {
                        [one] => <$t>::from_str(one).ok(),
                        _ => None,
                    }
                }
            }
        )*
    };
}

scalar_from_parts!(u8, u32, i64, bool, String);

macro_rules! tuple_from_parts {
    ($(($($name:ident),+))+) => {
        $(
            impl<$($name: FromStr),+> FromParts for ($($name),+) {
                fn from_parts(parts: &[&str]) -> Option<Self> {
                    let mut words = parts.iter();
                    let value = ($($name::from_str(words.next()?).ok()?),+);
                    if words.next().is_some() {
                        return None;
                    }
                    Some(value)
                }
            }
        )+
    };
}

tuple_from_parts! { (A) (A, B) (A, B, C) }

fn main() {
    println!("{:?}", <(String, u32)>::from_parts(&["ann", "5"]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_one_element_tuple_is_not_a_bare_value() {
        assert_eq!(<(u32,)>::from_parts(&["5"]), Some((5,)));
        assert_eq!(<(u32,)>::from_parts(&["5", "6"]), None);
        assert_eq!(<(u32,)>::from_parts(&[]), None);
        assert_eq!(<u32>::from_parts(&["7"]), Some(7), "одиночный тип остаётся одиночным");
    }

    #[test]
    fn pairs_and_triples_parse_field_by_field() {
        assert_eq!(<(String, u32)>::from_parts(&["ann", "5"]), Some(("ann".to_string(), 5)));
        assert_eq!(<(String, u32, bool)>::from_parts(&["ann", "5", "true"]), Some(("ann".to_string(), 5, true)));
    }

    #[test]
    fn wrong_counts_and_bad_words_give_none() {
        assert_eq!(<(String, u32)>::from_parts(&["ann"]), None, "слов меньше, чем полей");
        assert_eq!(<(String, u32)>::from_parts(&["ann", "5", "x"]), None, "лишние слова");
        assert_eq!(<(String, u32)>::from_parts(&["ann", "five"]), None, "слово не разобралось");
        assert_eq!(<(u8, u8)>::from_parts(&["1", "300"]), None);
    }

    #[test]
    fn four_fields_are_supported_too() {
        assert_eq!(<(u8, u8, u8, u8)>::from_parts(&["1", "2", "3", "4"]), Some((1, 2, 3, 4)));
        assert_eq!(<(i64, bool, String, u32)>::from_parts(&["-1", "false", "x", "9"]), Some((-1, false, "x".to_string(), 9)));
    }
}
