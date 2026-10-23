use std::fmt;

pub const MAX_TOKEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubjectError {
    Empty,
    TooLong,
    BadChar(char),
    NoTokens,
    BadWildcard,
}

impl fmt::Display for SubjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubjectError::Empty => write!(f, "пустое слово в теме"),
            SubjectError::TooLong => write!(f, "слово темы длиннее {MAX_TOKEN} символов"),
            SubjectError::BadChar(c) => write!(f, "недопустимый символ {c:?} в слове темы"),
            SubjectError::NoTokens => write!(f, "в теме нет ни одного слова"),
            SubjectError::BadWildcard => write!(f, "`>` допустим только последним словом шаблона"),
        }
    }
}

impl std::error::Error for SubjectError {}

pub fn token(raw: &str) -> Result<&str, SubjectError> {
    todo!()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Subject(String);

impl Subject {
    pub fn new(tokens: &[&str]) -> Result<Subject, SubjectError> {
        todo!()
    }

    pub fn parse(raw: &str) -> Result<Subject, SubjectError> {
        todo!()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn last_token(&self) -> &str {
        todo!()
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seg<'a> {
    Word(&'a str),
    Any,
    Tail,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pattern(String);

impl Pattern {
    pub fn build(segments: &[Seg<'_>]) -> Result<Pattern, SubjectError> {
        todo!()
    }

    pub fn parse(raw: &str) -> Result<Pattern, SubjectError> {
        todo!()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn matches(&self, subject: &Subject) -> bool {
        todo!()
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
