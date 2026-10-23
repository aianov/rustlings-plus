// В протоколе статусы пользователей ходят строками: `"active"`, `"banned"`. В Rust это enum, и для каждого
// такого enum нужны одни и те же вещи: перевод в строку и обратно, полный список, понятная ошибка.
// Писать их руками для каждого enum утомительно и ломается при добавлении варианта. Макрос `wire_enum!`
// описывает всё один раз:
//     wire_enum! {
//         /// Статус аккаунта.
//         pub enum Status { Active = "active", Banned = "banned" }
//     }
// и даёт: сам enum (с `Debug, Clone, Copy, PartialEq, Eq`; атрибуты и документацию из вызова нужно
// сохранить, видимость тоже), `Status::ALL` (константа-срез всех вариантов в порядке записи), `as_str()`,
// `Display` (то же, что `as_str`) и `FromStr` с ошибкой `UnknownVariant`. Текст ошибки:
// `unknown value "x", expected one of: active, banned`.
// TODO: Напиши `wire_enum!`.
#![allow(unused_macros)]

use std::fmt;
use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub struct UnknownVariant {
    pub input: String,
    pub expected: &'static [&'static str],
}

impl fmt::Display for UnknownVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown value {:?}, expected one of: {}", self.input, self.expected.join(", "))
    }
}

impl std::error::Error for UnknownVariant {}

macro_rules! wire_enum {
    () => {};
}

wire_enum! {
    /// Статус аккаунта.
    pub enum Status {
        /// Всё хорошо.
        Active = "active",
        Banned = "banned",
        Deleted = "deleted",
    }
}

wire_enum! {
    #[derive(Hash)]
    pub(crate) enum Channel { Email = "email", Push = "push" }
}

fn main() {
    println!("{}", Status::Active);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_variant_round_trips() {
        for status in Status::ALL {
            assert_eq!(status.as_str().parse::<Status>(), Ok(*status));
            assert_eq!(status.to_string(), status.as_str());
        }
        assert_eq!(Status::ALL, [Status::Active, Status::Banned, Status::Deleted]);
        assert_eq!("banned".parse::<Status>(), Ok(Status::Banned));
    }

    #[test]
    fn all_is_a_constant() {
        const COUNT: usize = Status::ALL.len();
        assert_eq!(COUNT, 3);
        const FIRST: Status = Status::ALL[0];
        assert_eq!(FIRST, Status::Active);
    }

    #[test]
    fn unknown_text_gets_a_helpful_error() {
        let err = "Active".parse::<Status>().unwrap_err();
        assert_eq!(err.input, "Active", "сравнение регистрозависимое");
        assert_eq!(err.to_string(), "unknown value \"Active\", expected one of: active, banned, deleted");
        let other = "sms".parse::<Channel>().unwrap_err();
        assert_eq!(other.to_string(), "unknown value \"sms\", expected one of: email, push");
    }

    #[test]
    fn attributes_from_the_call_are_kept() {
        let set: HashSet<Channel> = Channel::ALL.iter().copied().collect();
        assert_eq!(set.len(), 2, "derive(Hash) из вызова должен дойти до enum");
        assert_eq!(Status::Active, Status::Active.clone());
    }

    #[test]
    fn two_enums_in_one_file_do_not_clash() {
        assert_eq!(Channel::Push.as_str(), "push");
        assert_eq!(Status::Deleted.as_str(), "deleted");
    }
}
