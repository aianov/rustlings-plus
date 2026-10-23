use std::collections::BTreeMap;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum Error {
    BadLine { line: usize, text: String, source: ParseIntError },
    MissingSeparator { line: usize },
    Duplicate { name: String, line: usize },
    NoSuchAccount(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadLine { line, text, .. } => write!(f, "bad line {line}: {text:?}"),
            Error::MissingSeparator { line } => write!(f, "missing '=' on line {line}"),
            Error::Duplicate { name, line } => write!(f, "duplicate account {name} on line {line}"),
            Error::NoSuchAccount(name) => write!(f, "account {name} does not exist"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::BadLine { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub struct Store {
    accounts: BTreeMap<String, i64>,
}

impl Store {
    pub fn parse(text: &str) -> Result<Store> {
        let mut accounts = BTreeMap::new();
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let (name, amount) = trimmed.split_once('=').ok_or(Error::MissingSeparator { line })?;
            let amount: i64 = amount
                .trim()
                .parse()
                .map_err(|source| Error::BadLine { line, text: trimmed.to_string(), source })?;
            let name = name.trim().to_string();
            if accounts.insert(name.clone(), amount).is_some() {
                return Err(Error::Duplicate { name, line });
            }
        }
        Ok(Store { accounts })
    }

    pub fn get(&self, name: &str) -> Result<i64> {
        self.accounts.get(name).copied().ok_or_else(|| Error::NoSuchAccount(name.to_string()))
    }

    pub fn set(&mut self, name: &str, amount: i64) -> Result<()> {
        match self.accounts.get_mut(name) {
            Some(slot) => {
                *slot = amount;
                Ok(())
            }
            None => Err(Error::NoSuchAccount(name.to_string())),
        }
    }
}
