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
        todo!()
    }
}

impl<I: Iterator> PagesExt for I {
    fn pages(self, size: usize) -> Pages<Self> {
        todo!()
    }
}

impl Feed {
    pub fn new(posts: Vec<Post>) -> Feed {
        todo!("хранить по возрастанию created")
    }

    pub fn len(&self) -> usize {
        self.posts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.posts.is_empty()
    }

    pub fn visible<'a>(&'a self, hidden: &'a HashSet<String>) -> impl Iterator<Item = &'a Post> + 'a {
        todo!();
        std::iter::empty()
    }

    pub fn since<'a>(&'a self, hidden: &'a HashSet<String>, after: u64) -> impl Iterator<Item = &'a Post> + 'a {
        todo!();
        std::iter::empty()
    }

    pub fn authors<'a>(&'a self, hidden: &'a HashSet<String>) -> impl Iterator<Item = &'a str> + 'a {
        todo!();
        std::iter::empty()
    }

    pub fn paged<'a>(&'a self, hidden: &'a HashSet<String>, size: usize) -> Pages<impl Iterator<Item = &'a Post> + 'a> {
        todo!();
        std::iter::empty().pages(size)
    }

    pub fn page<'a>(&'a self, hidden: &'a HashSet<String>, number: usize, size: usize) -> Option<Vec<&'a Post>> {
        todo!()
    }
}
