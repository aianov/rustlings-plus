#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    pub title: String,
    pub pages: u32,
}

impl Book {
    pub fn new(title: &str, pages: u32) -> Self {
        Book {
            title: title.to_string(),
            pages,
        }
    }
}
