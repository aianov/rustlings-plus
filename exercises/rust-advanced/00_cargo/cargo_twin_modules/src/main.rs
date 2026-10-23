// Программа собирает полку из двух книг и печатает сводку из библиотеки.
// Но `cargo build` ругается на несовпадение типов, хотя оба называются Book.
// TODO: разберись, почему так, и почини. Тип не переименовывай и не копируй.
mod model;

use model::Book;

fn main() {
    let shelf = vec![Book::new("Programming Rust", 736), Book::new("Rust for Rustaceans", 280)];
    println!("{}", shelf::summary(&shelf));
}
