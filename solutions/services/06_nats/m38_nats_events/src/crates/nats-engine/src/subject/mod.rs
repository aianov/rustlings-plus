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
    if raw.is_empty() {
        return Err(SubjectError::Empty);
    }
    if raw.chars().count() > MAX_TOKEN {
        return Err(SubjectError::TooLong);
    }
    match raw
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '-'))
    {
        Some(bad) => Err(SubjectError::BadChar(bad)),
        None => Ok(raw),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Subject(String);

impl Subject {
    pub fn new(tokens: &[&str]) -> Result<Subject, SubjectError> {
        if tokens.is_empty() {
            return Err(SubjectError::NoTokens);
        }
        let checked: Result<Vec<&str>, SubjectError> = tokens.iter().map(|t| token(t)).collect();
        Ok(Subject(checked?.join(".")))
    }

    pub fn parse(raw: &str) -> Result<Subject, SubjectError> {
        let tokens: Vec<&str> = raw.split('.').collect();
        Subject::new(&tokens)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn last_token(&self) -> &str {
        self.0.rsplit('.').next().unwrap_or_default()
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
        if segments.is_empty() {
            return Err(SubjectError::NoTokens);
        }
        let mut parts = Vec::with_capacity(segments.len());
        for (i, segment) in segments.iter().enumerate() {
            parts.push(match segment {
                Seg::Word(word) => token(word)?,
                Seg::Any => "*",
                Seg::Tail if i + 1 == segments.len() => ">",
                Seg::Tail => return Err(SubjectError::BadWildcard),
            });
        }
        Ok(Pattern(parts.join(".")))
    }

    pub fn parse(raw: &str) -> Result<Pattern, SubjectError> {
        let segments: Vec<Seg<'_>> = raw
            .split('.')
            .map(|word| match word {
                "*" => Seg::Any,
                ">" => Seg::Tail,
                other => Seg::Word(other),
            })
            .collect();
        Pattern::build(&segments)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn matches(&self, subject: &Subject) -> bool {
        let mut want = self.0.split('.');
        let mut have = subject.0.split('.');
        loop {
            match (want.next(), have.next()) {
                (Some(">"), Some(_)) => return true,
                (Some("*"), Some(_)) => {}
                (Some(w), Some(h)) if w == h => {}
                (None, None) => return true,
                _ => return false,
            }
        }
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
