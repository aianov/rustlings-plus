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
    let mut tokens = Vec::new();
    let mut chars = src.char_indices().peekable();
    while let Some(&(at, c)) = chars.peek() {
        match c {
            ' ' | '\t' => {
                chars.next();
            }
            '+' | '-' | '*' | '(' | ')' => {
                chars.next();
                tokens.push(match c {
                    '+' => Token::Plus,
                    '-' => Token::Minus,
                    '*' => Token::Star,
                    '(' => Token::LParen,
                    _ => Token::RParen,
                });
            }
            '0'..='9' => {
                let mut n: i64 = 0;
                while let Some((_, d)) = chars.next_if(|(_, d)| d.is_ascii_digit()) {
                    let digit = i64::from(d.to_digit(10).unwrap_or(0));
                    n = n.checked_mul(10).and_then(|n| n.checked_add(digit)).ok_or_else(|| format!("number too large at {at}"))?;
                }
                tokens.push(Token::Num(n));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut name = String::new();
                while let Some((_, d)) = chars.next_if(|(_, d)| d.is_ascii_alphanumeric() || *d == '_') {
                    name.push(d);
                }
                tokens.push(Token::Ident(name));
            }
            other => return Err(format!("unexpected {other:?} at {at}")),
        }
    }
    Ok(tokens)
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
