// Времена жизни нужны и тогда, когда структуры хранят ссылки.

// TODO: Исправь ошибки компилятора, связанные со структурой.
struct Book {
    author: &str,
    title: &str,
}

fn main() {
    let book = Book {
        author: "George Orwell",
        title: "1984",
    };

    println!("{} (автор: {})", book.title, book.author);
}
