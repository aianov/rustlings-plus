use shelf::model::Book;

fn main() {
    let shelf = vec![Book::new("Programming Rust", 736), Book::new("Rust for Rustaceans", 280)];
    println!("{}", shelf::summary(&shelf));
}
