use ledger_core::{Bank, Error};
use std::error::Error as _;

const LEDGER: &str = "alice=100\nbob=20\n";

#[test]
fn transfer_moves_money() {
    let mut bank = Bank::open(LEDGER).unwrap();
    bank.transfer("alice", "bob", 30).unwrap();
    assert_eq!(bank.balance("alice").unwrap(), 70);
    assert_eq!(bank.balance("bob").unwrap(), 50);
}

#[test]
fn bad_amounts_and_missing_money() {
    let mut bank = Bank::open(LEDGER).unwrap();
    let err = bank.transfer("alice", "bob", 0).unwrap_err();
    assert!(matches!(err, Error::InvalidAmount(0)), "получили {err:?}");
    assert_eq!(err.to_string(), "amount must be positive, got 0");
    assert!(matches!(bank.transfer("alice", "bob", -4), Err(Error::InvalidAmount(-4))));
    let err = bank.transfer("alice", "bob", 500).unwrap_err();
    assert!(matches!(&err, Error::Insufficient { account, needed: 500, available: 100 } if account == "alice"), "получили {err:?}");
    assert_eq!(err.to_string(), "alice has 100, needs 500");
    assert!(err.source().is_none(), "у нехватки денег причины нет");
}

#[test]
fn a_failed_transfer_changes_nothing() {
    let mut bank = Bank::open(LEDGER).unwrap();
    assert!(bank.transfer("alice", "carol", 10).is_err());
    assert_eq!(bank.balance("alice").unwrap(), 100, "получателя нет: у отправителя ничего не списывается");
    assert!(bank.transfer("alice", "bob", 1000).is_err());
    assert_eq!(bank.balance("alice").unwrap(), 100);
    assert_eq!(bank.balance("bob").unwrap(), 20);
}

#[test]
fn transfer_to_yourself_keeps_the_balance() {
    let mut bank = Bank::open(LEDGER).unwrap();
    bank.transfer("alice", "alice", 30).unwrap();
    assert_eq!(bank.balance("alice").unwrap(), 100, "перевод самому себе остаётся нулевым");
    assert!(bank.transfer("alice", "alice", 300).is_err(), "нехватка денег действует и здесь");
}

#[test]
fn lower_layer_errors_are_wrapped_with_their_own_text() {
    let err = Bank::open("alice=1\nbob").err().unwrap();
    assert!(matches!(err, Error::Open { .. }), "получили {err:?}");
    assert_eq!(err.to_string(), "cannot open ledger");
    let cause = err.source().expect("причина: ошибка хранилища");
    assert_eq!(cause.to_string(), "missing '=' on line 2");
    assert!(cause.downcast_ref::<ledger_store::Error>().is_some(), "внутри настоящая ledger_store::Error");

    let bank = Bank::open(LEDGER).unwrap();
    let err = bank.balance("zed").unwrap_err();
    assert!(
        matches!(&err, Error::Lookup { source: ledger_store::Error::NoSuchAccount(n) } if n == "zed"),
        "причина не должна превращаться в строку: {err:?}"
    );
    assert_eq!(err.to_string(), "lookup failed");
    assert_eq!(err.source().unwrap().to_string(), "account zed does not exist");
}
