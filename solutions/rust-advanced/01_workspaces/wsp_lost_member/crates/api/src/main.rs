fn main() {
    let entries: Vec<i64> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    println!("ledger-api: total={}", ledger_core::total(&entries));
}
