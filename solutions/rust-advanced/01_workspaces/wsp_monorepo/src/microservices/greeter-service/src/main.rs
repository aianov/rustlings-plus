mod handler;

fn main() {
    let text = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    println!("{}", handler::greeting(&text));
}
