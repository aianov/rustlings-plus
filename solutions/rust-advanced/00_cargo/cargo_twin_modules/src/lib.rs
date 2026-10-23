pub mod model;

use model::Book;

pub fn summary(books: &[Book]) -> String {
    let pages: u32 = books.iter().map(|b| b.pages).sum();
    format!("{} books, {} pages", books.len(), pages)
}
