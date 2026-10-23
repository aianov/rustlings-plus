use std::fmt;
use std::fs;
use std::io;
use std::num::ParseIntError;
use std::path::Path;

#[derive(Debug)]
pub enum TotalError {
    Io(io::Error),
    Number(ParseIntError),
}

impl fmt::Display for TotalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TotalError::Io(_) => write!(f, "cannot read the file"),
            TotalError::Number(_) => write!(f, "line is not a number"),
        }
    }
}

impl std::error::Error for TotalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TotalError::Io(e) => Some(e),
            TotalError::Number(e) => Some(e),
        }
    }
}

impl From<io::Error> for TotalError {
    fn from(e: io::Error) -> TotalError {
        TotalError::Io(e)
    }
}

impl From<ParseIntError> for TotalError {
    fn from(e: ParseIntError) -> TotalError {
        TotalError::Number(e)
    }
}

pub fn total(path: &Path) -> Result<i64, TotalError> {
    let text = fs::read_to_string(path)?;
    let mut sum = 0;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        sum += line.parse::<i64>()?;
    }
    Ok(sum)
}

fn main() {
    println!("{:?}", total(Path::new("numbers.txt")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_file(content: &str) -> std::path::PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!("erp5_{}_{}.txt", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        fs::write(&path, content).expect("не удалось записать временный файл");
        path
    }

    #[test]
    fn sums_lines_and_skips_blanks() {
        let path = temp_file("10\n\n  5 \n-3\n");
        assert_eq!(total(&path).unwrap(), 12);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let err = total(Path::new("/definitely/not/here.txt")).unwrap_err();
        match err {
            TotalError::Io(e) => assert_eq!(e.kind(), io::ErrorKind::NotFound),
            other => panic!("ждали Io, получили {other:?}"),
        }
    }

    #[test]
    fn bad_line_is_a_number_error() {
        let path = temp_file("1\n2\nthree\n4\n");
        let err = total(&path).unwrap_err();
        assert!(matches!(err, TotalError::Number(_)), "строка three это ошибка числа, получили {err:?}");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn conversions_exist_for_both_causes() {
        let a: TotalError = io::Error::from(io::ErrorKind::PermissionDenied).into();
        assert!(matches!(a, TotalError::Io(_)));
        let b: TotalError = "x".parse::<i64>().unwrap_err().into();
        assert!(matches!(b, TotalError::Number(_)));
    }
}
