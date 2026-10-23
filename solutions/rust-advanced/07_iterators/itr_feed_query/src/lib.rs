use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u64,
    pub author: String,
    pub text: String,
    pub created: u64,
    pub deleted: bool,
}

pub struct Feed {
    posts: Vec<Post>,
}

pub struct Pages<I: Iterator> {
    inner: I,
    size: usize,
}

pub trait PagesExt: Iterator + Sized {
    fn pages(self, size: usize) -> Pages<Self>;
}

impl<I: Iterator> Iterator for Pages<I> {
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Vec<I::Item>> {
        let page: Vec<I::Item> = self.inner.by_ref().take(self.size).collect();
        (!page.is_empty()).then_some(page)
    }
}

impl<I: Iterator> PagesExt for I {
    fn pages(self, size: usize) -> Pages<Self> {
        assert!(size > 0, "page size must be positive");
        Pages { inner: self, size }
    }
}

impl Feed {
    pub fn new(mut posts: Vec<Post>) -> Feed {
        posts.sort_by_key(|p| p.created);
        Feed { posts }
    }

    pub fn len(&self) -> usize {
        self.posts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.posts.is_empty()
    }

    pub fn visible<'a>(&'a self, hidden: &'a HashSet<String>) -> impl Iterator<Item = &'a Post> + 'a {
        self.posts.iter().rev().filter(move |p| !p.deleted && !hidden.contains(p.author.as_str()))
    }

    pub fn since<'a>(&'a self, hidden: &'a HashSet<String>, after: u64) -> impl Iterator<Item = &'a Post> + 'a {
        self.posts.iter().rev().take_while(move |p| p.created > after).filter(move |p| !p.deleted && !hidden.contains(p.author.as_str()))
    }

    pub fn authors<'a>(&'a self, hidden: &'a HashSet<String>) -> impl Iterator<Item = &'a str> + 'a {
        let mut seen = HashSet::new();
        self.visible(hidden).map(|p| p.author.as_str()).filter(move |a| seen.insert(*a))
    }

    pub fn paged<'a>(&'a self, hidden: &'a HashSet<String>, size: usize) -> Pages<impl Iterator<Item = &'a Post> + 'a> {
        self.visible(hidden).pages(size)
    }

    pub fn page<'a>(&'a self, hidden: &'a HashSet<String>, number: usize, size: usize) -> Option<Vec<&'a Post>> {
        let start = number.checked_mul(size)?;
        let page: Vec<&'a Post> = self.visible(hidden).skip(start).take(size).collect();
        (!page.is_empty()).then_some(page)
    }
}
