// Верхний слой утилиты не хочет заводить enum на каждую мелочь: ему достаточно «что-то пошло не так, вот
// текст, а вот причина». Для этого есть `Box<dyn Error + Send + Sync>`: `?` кладёт в него любую стандартную
// ошибку, а строку превращает `.into()`. Бывает и обратное желание: узнать, не было ли в глубине цепочки
// конкретной причины, например «файла нет», чтобы тихо подставить значения по умолчанию.
// TODO: Допиши `read_limit` (файл с одним числом от 1 до 1000) и `is_missing_file`, которая смотрит
// не только на саму ошибку, но и на всю цепочку причин.
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug)]
pub struct Wrapped {
    pub source: io::Error,
}

impl fmt::Display for Wrapped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wrapped")
    }
}

impl Error for Wrapped {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

pub fn read_limit(path: &Path) -> Result<u32, Box<dyn Error + Send + Sync>> {
    todo!("прочитать файл, разобрать число, проверить диапазон")
}

pub fn is_missing_file(err: &(dyn Error + 'static)) -> bool {
    todo!("есть ли в цепочке io::Error с видом NotFound")
}

fn main() {
    println!("{:?}", read_limit(Path::new("limit.txt")).map_err(|e| e.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_file(content: &str) -> std::path::PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!("erp8_{}_{}.txt", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        fs::write(&path, content).expect("не удалось записать временный файл");
        path
    }

    #[test]
    fn reads_a_good_limit() {
        let path = temp_file(" 250\n");
        assert_eq!(read_limit(&path).unwrap(), 250);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn every_kind_of_failure_becomes_a_boxed_error() {
        let path = temp_file("0");
        assert_eq!(read_limit(&path).unwrap_err().to_string(), "limit must be positive");
        fs::write(&path, "5000").unwrap();
        assert_eq!(read_limit(&path).unwrap_err().to_string(), "limit 5000 is too large");
        fs::write(&path, "many").unwrap();
        assert_eq!(read_limit(&path).unwrap_err().to_string(), "invalid digit found in string");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn the_original_error_type_is_still_inside_the_box() {
        let err = read_limit(Path::new("/definitely/not/here.txt")).unwrap_err();
        let io_err = err.downcast_ref::<io::Error>().expect("внутри коробки настоящий io::Error");
        assert_eq!(io_err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn missing_file_is_found_at_any_depth() {
        let direct = read_limit(Path::new("/definitely/not/here.txt")).unwrap_err();
        assert!(is_missing_file(direct.as_ref()));
        let nested = Wrapped { source: io::Error::from(io::ErrorKind::NotFound) };
        assert!(is_missing_file(&nested), "причина лежит на уровень глубже, цепочку надо пройти");
    }

    #[test]
    fn other_causes_do_not_count() {
        let denied = Wrapped { source: io::Error::from(io::ErrorKind::PermissionDenied) };
        assert!(!is_missing_file(&denied), "нет прав это не «нет файла»");
        let text: Box<dyn Error + Send + Sync> = "plain text".into();
        assert!(!is_missing_file(text.as_ref()));
    }
}
