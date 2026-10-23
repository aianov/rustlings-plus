fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    let value: u8 = arg.parse().unwrap_or(0);
    println!("{}", value + 1);
}
