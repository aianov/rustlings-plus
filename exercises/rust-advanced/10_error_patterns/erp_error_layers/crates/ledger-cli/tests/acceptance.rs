use ledger_cli::{exit_code, report, run, Error};

const LEDGER: &str = "alice=100\nbob=20\n";

fn failure(ledger: &str, command: &str) -> (String, i32) {
    let err = run(ledger, command).expect_err("команда должна была не получиться");
    (report(err.as_ref()), exit_code(err.as_ref()))
}

#[test]
fn good_commands() {
    assert_eq!(run(LEDGER, "balance alice").unwrap(), "alice: 100");
    assert_eq!(run(LEDGER, "transfer alice bob 30").unwrap(), "transferred 30 from alice to bob");
    assert_eq!(run(LEDGER, "  balance   bob ").unwrap(), "bob: 20", "лишние пробелы не мешают");
}

#[test]
fn unknown_account_is_exit_3_with_the_whole_chain() {
    let (text, code) = failure(LEDGER, "balance carol");
    assert_eq!(text, "command failed: lookup failed: account carol does not exist");
    assert_eq!(code, 3, "код выбирается по корню цепочки: счёта нет");
}

#[test]
fn not_enough_money_is_exit_2() {
    let (text, code) = failure(LEDGER, "transfer alice bob 500");
    assert_eq!(text, "command failed: alice has 100, needs 500");
    assert_eq!(code, 2);
}

#[test]
fn bad_amount_text_comes_from_the_cli_layer() {
    let (text, code) = failure(LEDGER, "transfer alice bob ten");
    assert_eq!(text, "invalid amount \"ten\": invalid digit found in string");
    assert_eq!(code, 1);
    let (text, code) = failure(LEDGER, "transfer alice bob 0");
    assert_eq!(text, "command failed: amount must be positive, got 0");
    assert_eq!(code, 1);
}

#[test]
fn usage_is_checked_before_the_ledger_is_opened() {
    let (text, code) = failure("this is not a ledger", "dance");
    assert_eq!(text, "usage: balance <name> | transfer <from> <to> <amount>");
    assert_eq!(code, 64);
    let (_, code) = failure(LEDGER, "transfer alice bob");
    assert_eq!(code, 64);
}

#[test]
fn a_broken_ledger_shows_every_layer() {
    let (text, code) = failure("alice=100\nbob=x", "balance alice");
    assert_eq!(text, "command failed: cannot open ledger: bad line 2: \"bob=x\": invalid digit found in string");
    assert_eq!(code, 1);
    let (text, _) = failure("alice 100", "balance alice");
    assert_eq!(text, "command failed: cannot open ledger: missing '=' on line 1");
    let (text, _) = failure("a=1\na=2", "balance a");
    assert_eq!(text, "command failed: cannot open ledger: duplicate account a on line 2");
}

#[test]
fn the_boxed_error_still_knows_its_type() {
    let err = run(LEDGER, "dance").unwrap_err();
    assert!(matches!(err.downcast_ref::<Error>(), Some(Error::Usage(_))), "внутри коробки настоящая ledger_cli::Error");
    let handle = std::thread::spawn(move || run(LEDGER, "balance nobody").map_err(|e| e.to_string()));
    assert_eq!(handle.join().unwrap().unwrap_err(), "command failed");
}
