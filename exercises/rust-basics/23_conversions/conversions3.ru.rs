// В этом упражнении мы реализуем `FromStr`, чтобы превращать данные, хранящиеся
// в строке, в структурированный тип. В отличие от трейта `From`, преобразование
// через `FromStr` может завершиться неудачей, поэтому возвращает `Result`.
// Кроме того, реализовав `FromStr`, ты можешь вызывать у строк метод `parse`,
// чтобы получить объект реализующего типа. Подробнее об этом в
// документации:
// https://doc.rust-lang.org/std/str/trait.FromStr.html

use std::num::ParseIntError;
use std::str::FromStr;

#[derive(Debug, PartialEq)]
struct Person {
    name: String,
    age: u8,
}

// Этот тип ошибки мы используем в реализации `FromStr`.
#[derive(Debug, PartialEq)]
enum ParsePersonError {
    // Неверное число полей
    BadLen,
    // Пустое поле имени
    NoName,
    // Вложенная ошибка из parse::<u8>()
    ParseInt(ParseIntError),
}

// TODO: Допиши эту реализацию `FromStr`, чтобы из строки вида "Mark,20" можно
// было разобрать `Person`.
// Учти, что возраст нужно разобрать в `u8` чем-то вроде
// `"4".parse::<u8>()`.
//
// Шаги:
// 1. Раздели переданную строку по запятым.
// 2. Если разделение вернуло меньше или больше 2 элементов, верни ошибку
//    `ParsePersonError::BadLen`.
// 3. Первый элемент разделения используй как имя.
// 4. Если имя пустое, верни ошибку `ParsePersonError::NoName`.
// 5. Второй элемент разделения разбери в `u8` как возраст.
// 6. Если разбор возраста не удался, верни ошибку `ParsePersonError::ParseInt`.
impl FromStr for Person {
    type Err = ParsePersonError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {}
}

fn main() {
    let p = "Mark,20".parse::<Person>();
    println!("{p:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ParsePersonError::*;

    #[test]
    fn empty_input() {
        assert_eq!("".parse::<Person>(), Err(BadLen));
    }

    #[test]
    fn good_input() {
        let p = "John,32".parse::<Person>();
        assert!(p.is_ok());
        let p = p.unwrap();
        assert_eq!(p.name, "John");
        assert_eq!(p.age, 32);
    }

    #[test]
    fn missing_age() {
        assert!(matches!("John,".parse::<Person>(), Err(ParseInt(_))));
    }

    #[test]
    fn invalid_age() {
        assert!(matches!("John,twenty".parse::<Person>(), Err(ParseInt(_))));
    }

    #[test]
    fn missing_comma_and_age() {
        assert_eq!("John".parse::<Person>(), Err(BadLen));
    }

    #[test]
    fn missing_name() {
        assert_eq!(",1".parse::<Person>(), Err(NoName));
    }

    #[test]
    fn missing_name_and_age() {
        assert!(matches!(",".parse::<Person>(), Err(NoName | ParseInt(_))));
    }

    #[test]
    fn missing_name_and_invalid_age() {
        assert!(matches!(
            ",one".parse::<Person>(),
            Err(NoName | ParseInt(_)),
        ));
    }

    #[test]
    fn trailing_comma() {
        assert_eq!("John,32,".parse::<Person>(), Err(BadLen));
    }

    #[test]
    fn trailing_comma_and_some_string() {
        assert_eq!("John,32,man".parse::<Person>(), Err(BadLen));
    }
}
