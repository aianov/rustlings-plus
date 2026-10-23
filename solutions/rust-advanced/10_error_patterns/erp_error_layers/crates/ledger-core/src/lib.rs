use ledger_store::Store;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Open { source: ledger_store::Error },
    Lookup { source: ledger_store::Error },
    Insufficient { account: String, needed: i64, available: i64 },
    InvalidAmount(i64),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Open { .. } => write!(f, "cannot open ledger"),
            Error::Lookup { .. } => write!(f, "lookup failed"),
            Error::Insufficient { account, needed, available } => write!(f, "{account} has {available}, needs {needed}"),
            Error::InvalidAmount(n) => write!(f, "amount must be positive, got {n}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Open { source } | Error::Lookup { source } => Some(source),
            _ => None,
        }
    }
}

pub struct Bank {
    store: Store,
}

impl Bank {
    pub fn open(text: &str) -> Result<Bank> {
        let store = Store::parse(text).map_err(|source| Error::Open { source })?;
        Ok(Bank { store })
    }

    pub fn balance(&self, name: &str) -> Result<i64> {
        self.store.get(name).map_err(|source| Error::Lookup { source })
    }

    fn set(&mut self, name: &str, amount: i64) -> Result<()> {
        self.store.set(name, amount).map_err(|source| Error::Lookup { source })
    }

    pub fn transfer(&mut self, from: &str, to: &str, amount: i64) -> Result<()> {
        if amount <= 0 {
            return Err(Error::InvalidAmount(amount));
        }
        let available = self.balance(from)?;
        self.balance(to)?;
        if available < amount {
            return Err(Error::Insufficient { account: from.to_string(), needed: amount, available });
        }
        self.set(from, available - amount)?;
        let target = self.balance(to)?;
        self.set(to, target + amount)
    }
}
