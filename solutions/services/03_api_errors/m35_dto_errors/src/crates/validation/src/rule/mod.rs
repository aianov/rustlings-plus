mod url;

use std::error::Error;
use std::fmt;

use crate::pattern::{Pattern, PatternError};

pub use url::HttpUrl;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub code: &'static str,
    pub message: String,
}

impl Violation {
    pub fn new(code: &'static str, message: impl Into<String>) -> Violation {
        Violation {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl Error for Violation {}

pub trait Rule<T: ?Sized> {
    fn check(&self, value: &T) -> Result<(), Violation>;
}

impl<T: ?Sized, F> Rule<T> for F
where
    F: Fn(&T) -> Result<(), Violation>,
{
    fn check(&self, value: &T) -> Result<(), Violation> {
        self(value)
    }
}

pub struct Both<A, B> {
    first: A,
    second: B,
}

impl<T: ?Sized, A: Rule<T>, B: Rule<T>> Rule<T> for Both<A, B> {
    fn check(&self, value: &T) -> Result<(), Violation> {
        self.first.check(value)?;
        self.second.check(value)
    }
}

pub trait RuleExt<T: ?Sized>: Rule<T> + Sized {
    fn and<R: Rule<T>>(self, next: R) -> Both<Self, R> {
        Both {
            first: self,
            second: next,
        }
    }
}

impl<T: ?Sized, R: Rule<T>> RuleExt<T> for R {}

pub struct NotBlank;

impl Rule<str> for NotBlank {
    fn check(&self, value: &str) -> Result<(), Violation> {
        if value.trim().is_empty() {
            return Err(Violation::new("blank", "не может быть пустым"));
        }
        Ok(())
    }
}

pub struct CharLen {
    pub min: usize,
    pub max: usize,
}

impl Rule<str> for CharLen {
    fn check(&self, value: &str) -> Result<(), Violation> {
        let len = value.chars().count();
        if len < self.min {
            let message = format!("не короче {} символов, а здесь {len}", self.min);
            return Err(Violation::new("too_short", message));
        }
        if len > self.max {
            let message = format!("не длиннее {} символов, а здесь {len}", self.max);
            return Err(Violation::new("too_long", message));
        }
        Ok(())
    }
}

pub struct NoControl;

impl Rule<str> for NoControl {
    fn check(&self, value: &str) -> Result<(), Violation> {
        if value.chars().any(char::is_control) {
            let message = "переводы строк и другие управляющие символы запрещены";
            return Err(Violation::new("control_char", message));
        }
        Ok(())
    }
}

pub struct Matches {
    pub pattern: &'static str,
    pub what: &'static str,
}

impl Matches {
    pub fn compile(&self) -> Result<Pattern, PatternError> {
        Pattern::parse(self.pattern)
    }
}

impl Rule<str> for Matches {
    fn check(&self, value: &str) -> Result<(), Violation> {
        let Ok(pattern) = self.compile() else {
            let message = format!("шаблон {:?} не разбирается", self.pattern);
            return Err(Violation::new("bad_pattern", message));
        };
        if pattern.is_match(value) {
            return Ok(());
        }
        let message = format!("ждали {}", self.what);
        Err(Violation::new("invalid_format", message))
    }
}

pub struct Range<N> {
    pub min: N,
    pub max: N,
}

impl<N: PartialOrd + fmt::Display> Rule<N> for Range<N> {
    fn check(&self, value: &N) -> Result<(), Violation> {
        if *value < self.min || *value > self.max {
            let message = format!("от {} до {}", self.min, self.max);
            return Err(Violation::new("out_of_range", message));
        }
        Ok(())
    }
}

pub struct MaxItems(pub usize);

impl<T> Rule<[T]> for MaxItems {
    fn check(&self, value: &[T]) -> Result<(), Violation> {
        if value.len() > self.0 {
            let message = format!("не больше {}, а здесь {}", self.0, value.len());
            return Err(Violation::new("too_many", message));
        }
        Ok(())
    }
}

pub struct Unique;

impl<T: PartialEq> Rule<[T]> for Unique {
    fn check(&self, value: &[T]) -> Result<(), Violation> {
        let repeated = value
            .iter()
            .enumerate()
            .any(|(at, item)| value[..at].contains(item));
        if repeated {
            return Err(Violation::new("duplicate", "значения повторяются"));
        }
        Ok(())
    }
}

pub struct OneOf(pub &'static [&'static str]);

impl Rule<str> for OneOf {
    fn check(&self, value: &str) -> Result<(), Violation> {
        if self.0.contains(&value) {
            return Ok(());
        }
        let message = format!("одно из: {}", self.0.join(", "));
        Err(Violation::new("not_allowed", message))
    }
}
