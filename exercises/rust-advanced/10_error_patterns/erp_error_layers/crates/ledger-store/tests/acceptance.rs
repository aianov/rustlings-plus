use ledger_store::{Error, Store};
use std::error::Error as _;

#[test]
fn parses_accounts_and_skips_blank_lines() {
    let store = Store::parse("alice=100\n\n  bob = -5 \n").unwrap();
    assert_eq!(store.get("alice").unwrap(), 100);
    assert_eq!(store.get("bob").unwrap(), -5, "пробелы вокруг имени и суммы не мешают");
}

#[test]
fn bad_amount_knows_line_text_and_cause() {
    let err = Store::parse("alice=1\n\nbob=1o\n").err().expect("1o не число");
    match &err {
        Error::BadLine { line, text, .. } => {
            assert_eq!(*line, 3, "пустая строка тоже считается");
            assert_eq!(text, "bob=1o");
        }
        other => panic!("ждали BadLine, получили {other:?}"),
    }
    assert_eq!(err.to_string(), "bad line 3: \"bob=1o\"");
    let cause = err.source().expect("причина ParseIntError должна быть доступна через source");
    assert_eq!(cause.to_string(), "invalid digit found in string");
}

#[test]
fn structural_errors() {
    let err = Store::parse("alice 100").err().unwrap();
    assert!(matches!(err, Error::MissingSeparator { line: 1 }), "получили {err:?}");
    assert_eq!(err.to_string(), "missing '=' on line 1");
    assert!(err.source().is_none());
    let err = Store::parse("a=1\nb=2\na=3").err().unwrap();
    assert!(matches!(&err, Error::Duplicate { name, line: 3 } if name == "a"), "получили {err:?}");
    assert_eq!(err.to_string(), "duplicate account a on line 3");
}

#[test]
fn lookup_and_update() {
    let mut store = Store::parse("a=1").unwrap();
    store.set("a", 7).unwrap();
    assert_eq!(store.get("a").unwrap(), 7);
    let err = store.get("zed").unwrap_err();
    assert!(matches!(&err, Error::NoSuchAccount(n) if n == "zed"));
    assert_eq!(err.to_string(), "account zed does not exist");
    assert!(matches!(store.set("zed", 1), Err(Error::NoSuchAccount(_))), "set не создаёт счета");
}

#[test]
fn error_is_a_plain_std_error_that_can_cross_threads() {
    let err = Store::parse("x").err().unwrap();
    let text = std::thread::spawn(move || {
        let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(err);
        boxed.to_string()
    })
    .join()
    .unwrap();
    assert_eq!(text, "missing '=' on line 1");
    let alias: ledger_store::Result<i64> = Store::parse("a=1").unwrap().get("a");
    assert_eq!(alias.unwrap(), 1);
}
