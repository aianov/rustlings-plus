fn main() {
    let data = std::fs::read_to_string("data.txt").unwrap();
    println!("data: {}", data.trim());
}
