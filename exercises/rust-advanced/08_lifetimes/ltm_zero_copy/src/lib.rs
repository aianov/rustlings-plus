use std::borrow::Cow;
use std::ops::Range;

#[derive(Debug, PartialEq)]
pub struct LogError {
    pub line: usize,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry<'a> {
    pub ip: &'a str,
    pub method: &'a str,
    pub target: &'a str,
    pub status: u16,
    pub agent: Cow<'a, str>,
}

pub struct AccessLog {
    buffer: String,
    lines: Vec<Range<usize>>,
}

impl<'a> Entry<'a> {
    pub fn path(&self) -> &'a str {
        todo!("часть цели до ?")
    }

    pub fn query(&self) -> Option<&'a str> {
        todo!("часть цели после ?")
    }
}

impl AccessLog {
    pub fn new(buffer: String) -> Result<AccessLog, LogError> {
        todo!("проверить все строки и запомнить диапазоны")
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    pub fn raw(&self) -> &str {
        &self.buffer
    }

    pub fn get(&self, index: usize) -> Option<Entry<'_>> {
        todo!()
    }

    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> {
        std::iter::empty()
    }

    pub fn with_status(&self, min: u16, max: u16) -> impl Iterator<Item = Entry<'_>> {
        std::iter::empty()
    }
}
