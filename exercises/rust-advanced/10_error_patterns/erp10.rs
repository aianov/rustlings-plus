// В репозитории два «крейта», у них разные заботы. `storage` знает о строках формата `имя=сумма`, а `accounts`
// о правилах: нельзя снять больше, чем есть. Крейт `storage` уже написан: у него свой `Error` и свой
// алиас `Result`. Теперь так же нужно оформить `accounts`: свой `Error`, свой `Result`, а ошибки
// нижнего слоя должны превращаться в ошибки верхнего сами при использовании `?`, не теряя исходной.
// TODO: В модуле `accounts` заведи `Error` с вариантами `Storage(storage::Error)` и
// `Insufficient { needed: i64, available: i64 }`, алиас `Result`, `Display`, `Error` с `source` и `From`.
// Потом допиши `withdraw`: остаток после снятия. Тексты сообщений смотри в тестах.
pub mod storage {
    use std::fmt;

    #[derive(Debug, PartialEq)]
    pub enum Error {
        NotFound(String),
        Corrupted { line: usize },
    }

    pub type Result<T> = std::result::Result<T, Error>;

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Error::NotFound(name) => write!(f, "account {name} not found"),
                Error::Corrupted { line } => write!(f, "corrupted data on line {line}"),
            }
        }
    }

    impl std::error::Error for Error {}

    pub fn balance(raw: &str, name: &str) -> Result<i64> {
        let mut found = None;
        for (index, line) in raw.lines().enumerate() {
            let (key, value) = line.split_once('=').ok_or(Error::Corrupted { line: index + 1 })?;
            let value: i64 = value.trim().parse().map_err(|_| Error::Corrupted { line: index + 1 })?;
            if key.trim() == name {
                found = Some(value);
            }
        }
        found.ok_or_else(|| Error::NotFound(name.to_string()))
    }
}

pub mod accounts {
    use super::storage;

    pub fn withdraw(raw: &str, name: &str, amount: i64) -> i64 {
        todo!("остаток после снятия или ошибка")
    }
}

fn main() {
    println!("{:?}", storage::balance("a=10", "a"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn withdraws() {
        assert_eq!(accounts::withdraw("ann=10\nbob=3", "ann", 4).unwrap(), 6);
        assert_eq!(accounts::withdraw("ann=10", "ann", 10).unwrap(), 0, "снять всё можно");
    }

    #[test]
    fn insufficient_funds_has_its_own_variant() {
        let err = accounts::withdraw("ann=10", "ann", 30).unwrap_err();
        assert!(matches!(err, accounts::Error::Insufficient { needed: 30, available: 10 }), "получили {err:?}");
        assert_eq!(err.to_string(), "need 30, have 10");
        assert!(err.source().is_none());
    }

    #[test]
    fn lower_layer_errors_are_wrapped_not_flattened() {
        let err = accounts::withdraw("ann=10", "zed", 1).unwrap_err();
        assert!(matches!(err, accounts::Error::Storage(storage::Error::NotFound(ref n)) if n == "zed"), "получили {err:?}");
        assert_eq!(err.to_string(), "storage failed");
        let cause = err.source().expect("причина: ошибка хранилища");
        assert_eq!(cause.to_string(), "account zed not found");
        let broken = accounts::withdraw("ann=10\nnonsense", "ann", 1).unwrap_err();
        assert!(matches!(broken, accounts::Error::Storage(storage::Error::Corrupted { line: 2 })));
    }

    #[test]
    fn the_alias_is_accounts_own() {
        let value: accounts::Result<i64> = accounts::withdraw("a=5", "a", 1);
        assert_eq!(value.unwrap(), 4);
        let converted: accounts::Error = storage::Error::Corrupted { line: 9 }.into();
        assert!(matches!(converted, accounts::Error::Storage(_)));
    }
}
