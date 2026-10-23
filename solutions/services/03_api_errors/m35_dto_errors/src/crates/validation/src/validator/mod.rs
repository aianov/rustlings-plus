use std::error::Error;
use std::fmt;

use crate::rule::{Rule, Violation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub path: String,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    pub errors: Vec<FieldError>,
}

impl Invalid {
    pub fn single(path: &str, violation: Violation) -> Invalid {
        Invalid {
            errors: vec![FieldError {
                path: path.to_string(),
                code: violation.code,
                message: violation.message,
            }],
        }
    }

    pub fn pairs(&self) -> Vec<(&str, &str)> {
        self.errors
            .iter()
            .map(|error| (error.path.as_str(), error.code))
            .collect()
    }
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self
            .errors
            .iter()
            .map(|error| format!("{}: {} ({})", error.path, error.code, error.message))
            .collect();
        write!(f, "поля с ошибками: {}", parts.join("; "))
    }
}

impl Error for Invalid {}

pub fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Default)]
pub struct Validator {
    errors: Vec<FieldError>,
    path: Vec<String>,
}

impl Validator {
    pub fn new() -> Validator {
        Validator::default()
    }

    fn full_path(&self, field: &str) -> String {
        let mut out = String::new();
        for part in self.path.iter().map(String::as_str).chain([field]) {
            if part.is_empty() {
                continue;
            }
            if !out.is_empty() && !part.starts_with('[') {
                out.push('.');
            }
            out.push_str(part);
        }
        out
    }

    pub fn report(&mut self, field: &str, violation: Violation) {
        let path = self.full_path(field);
        self.errors.push(FieldError {
            path,
            code: violation.code,
            message: violation.message,
        });
    }

    pub fn check<T, R>(&mut self, field: &str, value: &T, rule: &R) -> bool
    where
        T: ?Sized,
        R: Rule<T> + ?Sized,
    {
        match rule.check(value) {
            Ok(()) => true,
            Err(violation) => {
                self.report(field, violation);
                false
            }
        }
    }

    pub fn parse<T>(&mut self, field: &str, raw: &str) -> Option<T>
    where
        T: for<'a> TryFrom<&'a str>,
        for<'a> <T as TryFrom<&'a str>>::Error: Into<Violation>,
    {
        match T::try_from(raw) {
            Ok(value) => Some(value),
            Err(error) => {
                self.report(field, error.into());
                None
            }
        }
    }

    pub fn nested(&mut self, field: &str, inside: impl FnOnce(&mut Validator)) {
        self.path.push(field.to_string());
        inside(self);
        self.path.pop();
    }

    pub fn each<T>(&mut self, field: &str, items: &[T], mut item: impl FnMut(&mut Validator, &T)) {
        self.path.push(field.to_string());
        for (index, value) in items.iter().enumerate() {
            self.path.push(format!("[{index}]"));
            item(self, value);
            self.path.pop();
        }
        self.path.pop();
    }

    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn finish_with<T>(self, value: Option<T>) -> Result<T, Invalid> {
        self.finish()?;
        value.ok_or_else(|| Invalid::single("", Violation::new("missing", "значение не собрано")))
    }

    pub fn finish(self) -> Result<(), Invalid> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(Invalid {
                errors: self.errors,
            })
        }
    }
}
