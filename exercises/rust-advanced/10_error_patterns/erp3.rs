// Загрузчик настроек оборачивает причины: файл не прочитался или строка не разобралась. `Display` говорит,
// что случилось на этом уровне, а настоящую причину (файла нет? нет прав? там буква вместо цифры?) должна
// отдавать сама ошибка, иначе вызывающий код вынужден разбирать текст сообщения. Сейчас причину не отдаёт
// ни один вариант.
// TODO: Сделай так, чтобы `source()` возвращал причину там, где она есть.
use std::error::Error;
use std::fmt;
use std::io;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum LoadError {
    Read { path: String, source: io::Error },
    Parse { line: usize, source: ParseIntError },
    Empty,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Read { path, .. } => write!(f, "cannot read {path}"),
            LoadError::Parse { line, .. } => write!(f, "bad number on line {line}"),
            LoadError::Empty => write!(f, "file is empty"),
        }
    }
}

impl Error for LoadError {}

fn main() {
    let err = LoadError::Empty;
    println!("{err}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_error_exposes_the_io_cause() {
        let err = LoadError::Read { path: "app.conf".into(), source: io::Error::from(io::ErrorKind::NotFound) };
        assert_eq!(err.to_string(), "cannot read app.conf");
        let cause = err.source().expect("у ошибки чтения есть причина: io::Error");
        let io_err = cause.downcast_ref::<io::Error>().expect("причина должна оставаться настоящим io::Error");
        assert_eq!(io_err.kind(), io::ErrorKind::NotFound, "по kind() код отличает «нет файла» от «нет прав»");
    }

    #[test]
    fn parse_error_exposes_the_number_cause() {
        let parse_err = "4x".parse::<i32>().unwrap_err();
        let err = LoadError::Parse { line: 3, source: parse_err.clone() };
        let cause = err.source().expect("у ошибки разбора есть причина: ParseIntError");
        assert_eq!(cause.downcast_ref::<ParseIntError>(), Some(&parse_err));
    }

    #[test]
    fn leaf_error_has_no_cause() {
        assert!(LoadError::Empty.source().is_none(), "у пустого файла причины нет");
    }

    #[test]
    fn works_behind_dyn_error() {
        let err: Box<dyn Error> = Box::new(LoadError::Parse { line: 1, source: "".parse::<u8>().unwrap_err() });
        assert!(err.source().is_some(), "через Box<dyn Error> причина тоже видна");
    }
}
