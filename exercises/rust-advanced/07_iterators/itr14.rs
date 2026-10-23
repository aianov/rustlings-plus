// Калькулятор читает выражения вроде `12 + x1*(y_2 - 3)`, и первый шаг это разбить текст на токены.
// Для этого нужно смотреть на символ вперёд: число `12` кончается не на `1`, а когда следующий символ уже
// не цифра, и этот символ пока читать нельзя. Такой подсмотр даёт `Peekable`: `peek` показывает следующее,
// не забирая его. Токены: `Num(i64)`, `Ident` (ASCII-буква или `_`, дальше ASCII-буквы, цифры, `_`), `Plus`, `Minus`,
// `Star`, `LParen`, `RParen`. Пробелы и табуляции пропускаются. Число и сразу идущие за ним буквы это два
// токена (`12ab` даёт `Num(12), Ident("ab")`). Неизвестный символ: ошибка `unexpected '$' at 2`, где
// число это номер байта в тексте. Слишком большое число: `number too large at <номер байта начала>`.
// TODO: Реализуй `tokenize` на `char_indices().peekable()`.

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Num(i64),
    Ident(String),
    Plus,
    Minus,
    Star,
    LParen,
    RParen,
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    todo!("токенизатор с подсмотром на символ вперёд")
}

fn main() {
    println!("{:?}", tokenize("1 + 2"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use Token::*;

    #[test]
    fn simple_expression() {
        assert_eq!(tokenize("12+x1*(y_2 - 3)"), Ok(vec![Num(12), Plus, Ident("x1".into()), Star, LParen, Ident("y_2".into()), Minus, Num(3), RParen]));
    }

    #[test]
    fn whitespace_and_empty() {
        assert_eq!(tokenize(""), Ok(vec![]));
        assert_eq!(tokenize(" \t "), Ok(vec![]));
        assert_eq!(tokenize("1\t+  2"), Ok(vec![Num(1), Plus, Num(2)]));
    }

    #[test]
    fn number_ends_where_digits_end() {
        assert_eq!(tokenize("12ab"), Ok(vec![Num(12), Ident("ab".into())]));
        assert_eq!(tokenize("007"), Ok(vec![Num(7)]));
        assert_eq!(tokenize("1(2"), Ok(vec![Num(1), LParen, Num(2)]), "символ после числа не потерян");
    }

    #[test]
    fn unknown_characters_report_the_byte_position() {
        assert_eq!(tokenize("3 $"), Err("unexpected '$' at 2".to_string()));
        assert_eq!(tokenize("ab é"), Err("unexpected 'é' at 3".to_string()), "позиция в байтах, а не в символах");
    }

    #[test]
    fn huge_numbers_are_rejected() {
        assert_eq!(tokenize("1 + 99999999999999999999"), Err("number too large at 4".to_string()));
        assert_eq!(tokenize("9223372036854775807"), Ok(vec![Num(i64::MAX)]));
    }
}
