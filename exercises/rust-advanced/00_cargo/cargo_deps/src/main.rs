fn main() {
    println!("{}", report_tool::VERSION);
    println!("{}", report_tool::render(&[("users", 42), ("posts", 1337)]));
}
