// Выписка разбирается по строкам вида `имя сумма`. Когда в третьей строке опечатка, ошибка говорит
// `invalid digit found in string`, и тот, у кого в выписке пятьсот строк, остаётся с этой фразой наедине.
// Ошибка должна нести номер строки (с единицы, пустые строки тоже считаются) и саму строку. Причина,
// то есть `ParseIntError`, остаётся доступной через `source`.
// TODO: Найди, откуда в ошибке берётся «ноль» вместо номера строки, и сделай так, чтобы вариант
// `BadLine` создавался только там, где известны и номер, и текст. Тест проверяет и это: голая `ParseIntError`
// через `?` в `StatementError` больше превращаться не должна.
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum StatementError {
    BadLine { line: usize, text: String, source: ParseIntError },
    MissingAmount { line: usize },
}

impl fmt::Display for StatementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatementError::BadLine { line, text, .. } => write!(f, "line {line}: cannot read amount from {text:?}"),
            StatementError::MissingAmount { line } => write!(f, "line {line}: no amount"),
        }
    }
}

impl std::error::Error for StatementError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StatementError::BadLine { source, .. } => Some(source),
            StatementError::MissingAmount { .. } => None,
        }
    }
}

impl From<ParseIntError> for StatementError {
    fn from(source: ParseIntError) -> StatementError {
        StatementError::BadLine { line: 0, text: String::new(), source }
    }
}

pub fn parse_statement(text: &str) -> Result<Vec<(String, i64)>, StatementError> {
    let mut rows = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let (name, amount) = line.split_once(' ').ok_or(StatementError::MissingAmount { line: index + 1 })?;
        rows.push((name.to_string(), amount.trim().parse()?));
    }
    Ok(rows)
}

fn main() {
    println!("{:?}", parse_statement("rent 500\nfood 120").map(|rows| rows.len()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

        const SRC: &str = include_str!("erp6.rs");

    fn compiles_with_lints(extra: &str, deny: &[&str]) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let mut cmd = std::process::Command::new("rustc");
        cmd.args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings"]);
        for lint in deny {
            cmd.args(["-D", lint]);
        }
        let ok = cmd.arg("--out-dir").arg(&dir).arg(&file).output().expect("не удалось запустить rustc").status.success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }


    #[test]
    fn parses_good_statements() {
        let rows = parse_statement("rent 500\n\nfood  -120\n").unwrap();
        assert_eq!(rows, vec![("rent".to_string(), 500), ("food".to_string(), -120)]);
    }

    #[test]
    fn error_knows_the_line_and_its_text() {
        let err = parse_statement("rent 500\n\nfood 12o\ngym 30").unwrap_err();
        match &err {
            StatementError::BadLine { line, text, .. } => {
                assert_eq!(*line, 3, "строки считаются с единицы, пустая вторая тоже строка");
                assert_eq!(text, "food 12o");
            }
            other => panic!("ждали BadLine, получили {other:?}"),
        }
        assert_eq!(err.to_string(), "line 3: cannot read amount from \"food 12o\"");
    }

    #[test]
    fn the_original_cause_survives() {
        let err = parse_statement("a x").unwrap_err();
        let cause = err.source().expect("ParseIntError остаётся причиной");
        assert!(cause.downcast_ref::<ParseIntError>().is_some());
    }

    #[test]
    fn missing_amount_has_a_line_too() {
        let err = parse_statement("rent 5\nlonely").unwrap_err();
        assert!(matches!(err, StatementError::MissingAmount { line: 2 }), "получили {err:?}");
    }

    #[test]
    fn no_blanket_conversion_from_a_bare_parse_error() {
        let extra = "pub fn leak(e: std::num::ParseIntError) -> StatementError { e.into() }";
        assert!(
            !compiles_with_lints(extra, &[]),
            "ParseIntError не должен сам превращаться в StatementError: у него нет номера строки"
        );
        assert!(compiles_with_lints("pub fn ok() {}", &[]), "проверка собирает остальной код");
    }
}
