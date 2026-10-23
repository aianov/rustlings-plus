use std::collections::BTreeMap;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum Error {
    BadLine { line: usize, text: String, source: ParseIntError },
    MissingSeparator { line: usize },
    Duplicate { name: String, line: usize },
    NoSuchAccount(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Store {
    accounts: BTreeMap<String, i64>,
}

impl Store {
    pub fn parse(text: &str) -> Result<Store> {
        todo!()
    }

    pub fn get(&self, name: &str) -> Result<i64> {
        todo!()
    }

    pub fn set(&mut self, name: &str, amount: i64) -> Result<()> {
        todo!()
    }
}
