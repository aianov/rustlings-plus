use ledger_store::Store;

#[derive(Debug)]
pub enum Error {
    Open { source: ledger_store::Error },
    Lookup { source: ledger_store::Error },
    Insufficient { account: String, needed: i64, available: i64 },
    InvalidAmount(i64),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Bank {
    store: Store,
}

impl Bank {
    pub fn open(text: &str) -> Result<Bank> {
        todo!()
    }

    pub fn balance(&self, name: &str) -> Result<i64> {
        todo!()
    }

    pub fn transfer(&mut self, from: &str, to: &str, amount: i64) -> Result<()> {
        todo!()
    }
}
