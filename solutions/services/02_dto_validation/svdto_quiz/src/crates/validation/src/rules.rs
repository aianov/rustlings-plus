use crate::pattern::{Pattern, PatternError};
use std::fmt::Display;

pub trait Rule<T: ?Sized> {
    fn check(&self, value: &T) -> Option<(&'static str, String)>;
}

pub struct NotBlank;

impl Rule<str> for NotBlank {
    fn check(&self, value: &str) -> Option<(&'static str, String)> {
        value.trim().is_empty().then(|| ("blank", "значение не должно быть пустым".to_string()))
    }
}

pub struct CharLen {
    pub min: usize,
    pub max: usize,
}

impl Rule<str> for CharLen {
    fn check(&self, value: &str) -> Option<(&'static str, String)> {
        let len = value.trim().chars().count();
        if len < self.min {
            return Some(("too_short", format!("минимум {} символов, получено {len}", self.min)));
        }
        if len > self.max {
            return Some(("too_long", format!("максимум {} символов, получено {len}", self.max)));
        }
        None
    }
}

pub struct Matches(Pattern);

impl Matches {
    pub fn new(pattern: &str) -> Result<Matches, PatternError> {
        Pattern::parse(pattern).map(Matches)
    }
}

impl Rule<str> for Matches {
    fn check(&self, value: &str) -> Option<(&'static str, String)> {
        (!self.0.is_match(value.trim())).then(|| ("invalid_format", "значение не подходит под формат".to_string()))
    }
}

pub struct Range<N> {
    pub min: N,
    pub max: N,
}

impl<N: PartialOrd + Display + Copy> Rule<N> for Range<N> {
    fn check(&self, value: &N) -> Option<(&'static str, String)> {
        (*value < self.min || *value > self.max)
            .then(|| ("out_of_range", format!("допустимо от {} до {}, получено {value}", self.min, self.max)))
    }
}

pub struct MaxItems(pub usize);

impl<T> Rule<[T]> for MaxItems {
    fn check(&self, value: &[T]) -> Option<(&'static str, String)> {
        (value.len() > self.0).then(|| ("too_many", format!("не больше {} элементов, получено {}", self.0, value.len())))
    }
}

pub struct OneOf(pub &'static [&'static str]);

impl Rule<str> for OneOf {
    fn check(&self, value: &str) -> Option<(&'static str, String)> {
        (!self.0.contains(&value)).then(|| ("not_allowed", format!("допустимо одно из: {}", self.0.join(", "))))
    }
}
