
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    PostNotFound,
    PostTooLong,
    Internal,
}

#[derive(Debug)]
pub enum PostError {
    NotFound { id: u64 },
    TooLong { len: usize, max: usize },
    Internal(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for PostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PostError::NotFound { id } => write!(f, "пост {id} не найден"),
            PostError::TooLong { len, max } => write!(f, "текст слишком длинный: {len} из {max}"),
            PostError::Internal(_) => write!(f, "внутренняя ошибка"),
        }
    }
}

impl Error for PostError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            PostError::Internal(cause) => Some(cause.as_ref()),
            _ => None,
        }
    }
}

impl PostError {
    pub fn code(&self) -> Code {
        match self {
            PostError::NotFound { .. } => Code::PostNotFound,
            PostError::TooLong { .. } => Code::PostTooLong,
            PostError::Internal(_) => Code::Internal,
        }
    }
}

fn main() {
    let err = PostError::TooLong { len: 300, max: 280 };
    println!("{err} ({:?})", err.code());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Cause(&'static str);

    impl fmt::Display for Cause {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl Error for Cause {}

    fn internal(text: &'static str) -> PostError {
        PostError::Internal(Box::new(Cause(text)))
    }

    #[test]
    fn display_names_the_facts() {
        let text = PostError::NotFound { id: 42 }.to_string();
        assert!(text.contains("42"), "сообщение должно называть номер поста, а не просто «не найден»: {text}");
        let text = PostError::TooLong { len: 300, max: 280 }.to_string();
        assert!(text.contains("300") && text.contains("280"), "нужны и длина, и предел: {text}");
    }

    #[test]
    fn display_of_internal_does_not_repeat_the_cause() {
        let text = internal("connection refused: db-7.internal:5432").to_string();
        assert!(!text.contains("db-7"), "Display попадёт в ответы и логи верхних слоёв, причину отдавай через source(): {text}");
        assert!(!text.is_empty(), "но сообщение не должно быть пустым");
    }

    #[test]
    fn source_chain_is_available() {
        let err = internal("disk full");
        let source = err.source().expect("у Internal должна быть причина");
        assert_eq!(source.to_string(), "disk full", "source() отдаёт исходную ошибку");
        assert!(source.downcast_ref::<Cause>().is_some(), "тип причины сохраняется, её можно опознать");
        assert!(PostError::NotFound { id: 1 }.source().is_none(), "у доменных ошибок причины нет");
        assert!(PostError::TooLong { len: 2, max: 1 }.source().is_none(), "и у этой тоже");
    }

    #[test]
    fn codes_follow_the_variant() {
        assert_eq!(PostError::NotFound { id: 1 }.code(), Code::PostNotFound, "NotFound");
        assert_eq!(PostError::TooLong { len: 2, max: 1 }.code(), Code::PostTooLong, "TooLong");
        assert_eq!(internal("x").code(), Code::Internal, "Internal");
    }

    #[test]
    fn can_travel_between_threads_and_trait_objects() {
        fn needs_send_sync<T: Send + Sync + 'static>(_: &T) {}
        let err = internal("x");
        needs_send_sync(&err);
        let boxed: Box<dyn Error + Send + Sync> = Box::new(err);
        assert_eq!(boxed.to_string(), "внутренняя ошибка", "через Box<dyn Error> печатается то же сообщение");
    }
}
