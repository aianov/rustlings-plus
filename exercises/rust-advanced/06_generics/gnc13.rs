// Пост собирается из многих необязательных частей и нескольких обязательных. Конструктор с шестью
// аргументами читать невозможно, поэтому делаем builder: цепочка вызовов `Post::builder().author("ann")
// .text("hi").tag("rust").build()`, где каждый метод берёт `self` и возвращает `Self`. Обязательные
// автор и текст проверяются в `build`, а не молча подставляются пустыми. Правила, в порядке проверки:
// нет автора (пустой после обрезки пробелов тоже) это `MissingAuthor`; нет текста это `MissingText`;
// текст длиннее 280 символов это `TextTooLong(сколько символов)`; больше пяти тегов это `TooManyTags`.
// Повторный тег не добавляется. Теги хранятся без `#` и в нижнем регистре. Флаг `pinned` по умолчанию ложь.
// TODO: Реализуй `PostBuilder` и `Post::builder`. Методы принимают `impl Into<String>`.

#[derive(Debug, PartialEq)]
pub struct Post {
    pub author: String,
    pub text: String,
    pub tags: Vec<String>,
    pub pinned: bool,
}

#[derive(Debug, PartialEq)]
pub enum BuildError {
    MissingAuthor,
    MissingText,
    TextTooLong(usize),
    TooManyTags,
}

#[derive(Default)]
pub struct PostBuilder {}

impl Post {
    pub fn builder() -> PostBuilder {
        todo!()
    }
}

impl PostBuilder {
    pub fn author(self, author: impl Into<String>) -> Self {
        todo!()
    }

    pub fn text(self, text: impl Into<String>) -> Self {
        todo!()
    }

    pub fn tag(self, tag: impl Into<String>) -> Self {
        todo!()
    }

    pub fn pinned(self, pinned: bool) -> Self {
        todo!()
    }

    pub fn build(self) -> Result<Post, BuildError> {
        todo!()
    }
}

fn main() {
    let post = Post::builder().author("ann").text("hello").build();
    println!("{post:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_post() {
        let post = Post::builder().author("ann").text("hi").build().expect("автор и текст заданы");
        assert_eq!(post, Post { author: "ann".into(), text: "hi".into(), tags: vec![], pinned: false });
    }

    #[test]
    fn order_of_calls_does_not_matter() {
        let a = Post::builder().text("hi").pinned(true).author(String::from("ann")).build();
        let b = Post::builder().author("ann").pinned(true).text("hi").build();
        assert_eq!(a, b);
        assert!(a.expect("собирается").pinned);
    }

    #[test]
    fn tags_are_normalized_and_deduplicated() {
        let post = Post::builder().author("a").text("t").tag("#Rust").tag("rust").tag("Async").build().expect("теги в порядке");
        assert_eq!(post.tags, vec!["rust", "async"], "без решётки, в нижнем регистре, без повторов");
    }

    #[test]
    fn missing_parts_are_errors() {
        assert_eq!(Post::builder().text("hi").build(), Err(BuildError::MissingAuthor));
        assert_eq!(Post::builder().author("  ").text("hi").build(), Err(BuildError::MissingAuthor), "пробелы вместо автора");
        assert_eq!(Post::builder().author("ann").build(), Err(BuildError::MissingText));
        assert_eq!(Post::builder().build(), Err(BuildError::MissingAuthor), "при двух ошибках сначала автор");
    }

    #[test]
    fn limits() {
        let ok = "я".repeat(280);
        assert!(Post::builder().author("a").text(ok).build().is_ok(), "ровно 280 символов допустимо");
        let long = "я".repeat(281);
        assert_eq!(Post::builder().author("a").text(long).build(), Err(BuildError::TextTooLong(281)), "длина считается в символах, не в байтах");
        let many = (1..=6).fold(Post::builder().author("a").text("t"), |b, n| b.tag(format!("t{n}")));
        assert_eq!(many.build(), Err(BuildError::TooManyTags));
        let five = (1..=5).fold(Post::builder().author("a").text("t"), |b, n| b.tag(format!("t{n}")));
        assert!(five.build().is_ok(), "пять тегов допустимо");
    }
}
