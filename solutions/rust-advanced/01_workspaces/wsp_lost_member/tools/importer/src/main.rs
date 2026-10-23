fn main() {
    let rows: Vec<i64> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    println!("imported {} rows, total {}", rows.len(), ledger_core::total(&rows));
}
