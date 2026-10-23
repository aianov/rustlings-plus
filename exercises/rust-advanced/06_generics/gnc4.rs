// Три функции принимают текст и байты, но придирчивы к типу: одна хочет только `&str`, другая только
// `&String`, третья только `&Vec<u8>`. Вызывающему приходится клонировать и приводить типы ради сигнатуры:
// тесты передают `String`, `&str`, `char`, `Cow<str>`, массивы и срезы, и ничего из этого не собирается.
// TODO: Перепиши сигнатуры так, чтобы они принимали всё, что естественно считать «текстом» или «байтами».
// `greet` забирает владение именем (складывает его в `String`), остальные только читают. Тела почти не меняются.

pub struct Greeting {
    pub text: String,
}

pub fn greet(name: &str) -> Greeting {
    Greeting { text: format!("Hello, {}!", name) }
}

pub fn count_vowels(s: &String) -> usize {
    s.chars().filter(|c| "aeiouAEIOU".contains(*c)).count()
}

pub fn checksum(data: &Vec<u8>) -> u8 {
    data.iter().fold(0u8, |acc, b| acc.wrapping_add(*b))
}

fn main() {
    println!("{}", greet("ann").text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn greet_takes_any_string_like() {
        assert_eq!(greet("ann").text, "Hello, ann!");
        assert_eq!(greet(String::from("bob")).text, "Hello, bob!");
        assert_eq!(greet('x').text, "Hello, x!", "char тоже превращается в String");
        assert_eq!(greet(Cow::Borrowed("cat")).text, "Hello, cat!");
        let owned = String::from("dan");
        assert_eq!(greet(owned.clone()).text, "Hello, dan!");
    }

    #[test]
    fn vowels_on_every_text_type() {
        let s = String::from("Education");
        assert_eq!(count_vowels("Education"), 5);
        assert_eq!(count_vowels(&s), 5, "&String должен подходить без .as_str()");
        assert_eq!(count_vowels(s.clone()), 5);
        assert_eq!(count_vowels(Cow::from("xyz")), 0);
        assert_eq!(count_vowels(s), 5, "и владение тоже принимается");
    }

    #[test]
    fn checksum_on_every_byte_type() {
        assert_eq!(checksum(vec![1u8, 2, 3]), 6);
        assert_eq!(checksum([250u8, 10]), 4, "сумма с переполнением по модулю 256");
        assert_eq!(checksum(&[1u8, 1][..]), 2);
        assert_eq!(checksum("AB"), b'A'.wrapping_add(b'B'), "строка это тоже байты");
        assert_eq!(checksum(b"\x01\x02"), 3);
        assert_eq!(checksum(String::from("a")), b'a');
        let data = vec![9u8; 3];
        assert_eq!(checksum(&data), 27, "&Vec<u8> без клонирования");
        assert_eq!(checksum(data), 27);
    }
}
