use ledger_core::Bank;
use std::error::Error as StdError;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum Error {
    Usage(String),
    BadAmount { text: String, source: ParseIntError },
    Core(ledger_core::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

const USAGE: &str = "balance <name> | transfer <from> <to> <amount>";

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(_) => write!(f, "usage: {USAGE}"),
            Error::BadAmount { text, .. } => write!(f, "invalid amount {text:?}"),
            Error::Core(_) => write!(f, "command failed"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::BadAmount { source, .. } => Some(source),
            Error::Core(source) => Some(source),
            Error::Usage(_) => None,
        }
    }
}

impl From<ledger_core::Error> for Error {
    fn from(e: ledger_core::Error) -> Error {
        Error::Core(e)
    }
}

fn execute(ledger: &str, command: &str) -> Result<String> {
    let words: Vec<&str> = command.split_whitespace().collect();
    match words.as_slice() {
        ["balance", name] => {
            let bank = Bank::open(ledger)?;
            Ok(format!("{name}: {}", bank.balance(name)?))
        }
        ["transfer", from, to, amount] => {
            let amount: i64 = amount
                .parse()
                .map_err(|source| Error::BadAmount { text: amount.to_string(), source })?;
            let mut bank = Bank::open(ledger)?;
            bank.transfer(from, to, amount)?;
            Ok(format!("transferred {amount} from {from} to {to}"))
        }
        _ => Err(Error::Usage(command.to_string())),
    }
}

pub fn run(ledger: &str, command: &str) -> std::result::Result<String, Box<dyn StdError + Send + Sync>> {
    execute(ledger, command).map_err(Into::into)
}

pub fn report(err: &(dyn StdError + 'static)) -> String {
    let mut parts = vec![err.to_string()];
    let mut current = err.source();
    while let Some(cause) = current {
        parts.push(cause.to_string());
        current = cause.source();
    }
    parts.join(": ")
}

pub fn exit_code(err: &(dyn StdError + 'static)) -> i32 {
    let mut current = Some(err);
    while let Some(e) = current {
        if let Some(ledger_core::Error::Insufficient { .. }) = e.downcast_ref::<ledger_core::Error>() {
            return 2;
        }
        if let Some(ledger_store::Error::NoSuchAccount(_)) = e.downcast_ref::<ledger_store::Error>() {
            return 3;
        }
        if let Some(Error::Usage(_)) = e.downcast_ref::<Error>() {
            return 64;
        }
        current = e.source();
    }
    1
}
