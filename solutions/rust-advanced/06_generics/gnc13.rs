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
pub struct PostBuilder {
    author: Option<String>,
    text: Option<String>,
    tags: Vec<String>,
    pinned: bool,
}

impl Post {
    pub fn builder() -> PostBuilder {
        PostBuilder::default()
    }
}

impl PostBuilder {
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        let tag = tag.into().trim_start_matches('#').to_lowercase();
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
        self
    }

    pub fn pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }

    pub fn build(self) -> Result<Post, BuildError> {
        let author = self.author.map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).ok_or(BuildError::MissingAuthor)?;
        let text = self.text.ok_or(BuildError::MissingText)?;
        let length = text.chars().count();
        if length > 280 {
            return Err(BuildError::TextTooLong(length));
        }
        if self.tags.len() > 5 {
            return Err(BuildError::TooManyTags);
        }
        Ok(Post { author, text, tags: self.tags, pinned: self.pinned })
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
