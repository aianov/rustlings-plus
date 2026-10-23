use std::error::Error as StdError;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum Error {
    Usage(String),
    BadAmount { text: String, source: ParseIntError },
    Core(ledger_core::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

const USAGE: &str = "balance <name> | transfer <from> <to> <amount>";

pub fn run(ledger: &str, command: &str) -> std::result::Result<String, Box<dyn StdError + Send + Sync>> {
    todo!()
}

pub fn report(err: &(dyn StdError + 'static)) -> String {
    todo!()
}

pub fn exit_code(err: &(dyn StdError + 'static)) -> i32 {
    todo!()
}
