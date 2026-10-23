use std::fmt::Display;
use std::str::FromStr;

pub fn parse_or<T: FromStr>(text: &str, default: T) -> T {
    text.trim().parse().unwrap_or(default)
}

pub fn parse_list<T>(text: &str) -> Result<Vec<T>, String>
where
    T: FromStr,
    T::Err: Display,
{
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    text.split(',')
        .enumerate()
        .map(|(i, item)| item.trim().parse::<T>().map_err(|e| format!("item {}: {e}", i + 1)))
        .collect()
}

#[derive(Debug, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl FromStr for Rgb {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let hex = s.strip_prefix('#').ok_or("нужен префикс #")?;
        if hex.len() != 6 {
            return Err(format!("нужно 6 цифр, получено {}", hex.len()));
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string());
        Ok(Rgb(byte(0)?, byte(2)?, byte(4)?))
    }
}

fn main() {
    println!("{}", parse_or("7", 1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_or_for_several_types() {
        assert_eq!(parse_or("42", 0), 42);
        assert_eq!(parse_or(" 42 ", 0), 42, "пробелы по краям не мешают");
        assert_eq!(parse_or("oops", 7u8), 7);
        assert_eq!(parse_or("2.5", 1.0), 2.5);
        assert!(parse_or("true", false));
        assert_eq!(parse_or("#0a0b0c", Rgb(0, 0, 0)), Rgb(10, 11, 12), "работает и для своего типа");
        assert_eq!(parse_or("300", 1u8), 1, "300 не влезает в u8: запасное значение");
    }

    #[test]
    fn parse_list_of_numbers() {
        assert_eq!(parse_list::<i32>("1, 2 ,3"), Ok(vec![1, 2, 3]));
        assert_eq!(parse_list::<f64>("0.5,1.5"), Ok(vec![0.5, 1.5]));
        assert_eq!(parse_list::<u8>(""), Ok(vec![]), "пустой текст это пустой список");
        assert_eq!(parse_list::<bool>("true,false"), Ok(vec![true, false]));
    }

    #[test]
    fn parse_list_reports_the_failing_position() {
        let err = parse_list::<i32>("1, 2, x, 4").expect_err("третий элемент не число");
        assert!(err.starts_with("item 3: "), "номер элемента с единицы, получено: {err}");
        assert!(err.contains("invalid digit"), "в сообщении должен быть текст ошибки разбора: {err}");
        let err = parse_list::<Rgb>("#010203,red").expect_err("второй элемент не цвет");
        assert_eq!(err, "item 2: нужен префикс #", "ошибка своего типа попадает в сообщение как есть");
    }

    #[test]
    fn parse_list_works_for_custom_types() {
        assert_eq!(parse_list::<Rgb>("#ff0000, #00ff00"), Ok(vec![Rgb(255, 0, 0), Rgb(0, 255, 0)]));
    }
}
