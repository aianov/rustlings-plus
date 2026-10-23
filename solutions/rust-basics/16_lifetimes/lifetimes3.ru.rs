// Времена жизни нужны и тогда, когда структуры хранят ссылки.

struct Book<'a> {
    //     ^^^^ добавлена аннотация времени жизни
    author: &'a str,
    //       ^^
    title: &'a str,
    //      ^^
}

fn main() {
    let book = Book {
        author: "George Orwell",
        title: "1984",
    };

    println!("{} (автор: {})", book.title, book.author);
}
