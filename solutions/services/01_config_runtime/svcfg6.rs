use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub struct DotenvError {
    pub line: usize,
}

pub fn parse_dotenv(text: &str) -> Result<HashMap<String, String>, DotenvError> {
    let mut out = HashMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            return Err(DotenvError { line: index + 1 });
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(DotenvError { line: index + 1 });
        }
        out.insert(key.to_string(), unquote(value.trim()));
    }
    Ok(out)
}

fn unquote(value: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(inner) = value.strip_prefix(quote).and_then(|v| v.strip_suffix(quote)) {
            return inner.to_string();
        }
    }
    match value.find(" #") {
        Some(at) => value[..at].trim_end().to_string(),
        None => value.to_string(),
    }
}

pub fn layered<'a>(
    file: &'a HashMap<String, String>,
    real: impl Fn(&str) -> Option<String> + 'a,
) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| real(name).or_else(|| file.get(name).cloned())
}

fn main() {
    let file = parse_dotenv("PORT=8080").unwrap();
    let source = layered(&file, |n| std::env::var(n).ok());
    println!("{:?}", source("PORT"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_pairs() {
        let env = parse_dotenv("PORT=8080\nHOST = localhost \n").unwrap();
        assert_eq!(env["PORT"], "8080", "обычная пара");
        assert_eq!(env["HOST"], "localhost", "пробелы вокруг ключа и значения срезаются");
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let env = parse_dotenv("# настройки\n\n   # ещё\nA=1\n").unwrap();
        assert_eq!(env.len(), 1, "комментарии и пустые строки не дают ключей");
        assert_eq!(env["A"], "1", "обычная пара после комментариев");
    }

    #[test]
    fn equals_sign_inside_the_value_is_kept() {
        let env = parse_dotenv("DATABASE_URL=postgres://u:p@h/db?sslmode=require&x=1").unwrap();
        assert_eq!(
            env["DATABASE_URL"],
            "postgres://u:p@h/db?sslmode=require&x=1",
            "делим строку только по первому `=`"
        );
    }

    #[test]
    fn quotes_protect_spaces_and_hashes() {
        let env = parse_dotenv("A=\"hello  world # not a comment\"\nB='single quoted'\nC=plain # tail\n").unwrap();
        assert_eq!(env["A"], "hello  world # not a comment", "в двойных кавычках всё остаётся как есть");
        assert_eq!(env["B"], "single quoted", "одинарные кавычки тоже снимаются");
        assert_eq!(env["C"], "plain", "хвостовой комментарий вне кавычек отбрасывается");
    }

    #[test]
    fn export_prefix_is_allowed() {
        let env = parse_dotenv("export TOKEN=abc").unwrap();
        assert_eq!(env["TOKEN"], "abc", "`export ` в начале строки игнорируется");
    }

    #[test]
    fn broken_line_reports_its_number() {
        let err = parse_dotenv("A=1\nвот это не пара\nB=2").unwrap_err();
        assert_eq!(err, DotenvError { line: 2 }, "ошибка называет номер строки (с единицы)");
        assert!(parse_dotenv("=value").is_err(), "пустой ключ это ошибка");
    }

    #[test]
    fn real_environment_beats_the_file() {
        let file = parse_dotenv("PORT=1\nONLY_IN_FILE=yes").unwrap();
        let source = layered(&file, |name| (name == "PORT").then(|| "2".to_string()));
        assert_eq!(source("PORT").as_deref(), Some("2"), "переменная окружения важнее файла");
        assert_eq!(source("ONLY_IN_FILE").as_deref(), Some("yes"), "чего нет в окружении, берём из файла");
        assert_eq!(source("NOWHERE"), None, "нигде нет: None");
    }
}
