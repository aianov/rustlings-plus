use std::iter::Fuse;

pub struct Pages<I: Iterator> {
    inner: Fuse<I>,
    size: usize,
}

pub trait PagesExt: Iterator + Sized {
    fn pages(self, size: usize) -> Pages<Self>;
}

impl<I: Iterator> PagesExt for I {
    fn pages(self, size: usize) -> Pages<I> {
        Pages {
            inner: self.fuse(),
            size,
        }
    }
}

impl<I: Iterator> Iterator for Pages<I> {
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Vec<I::Item>> {
        let room = self.inner.size_hint().1.unwrap_or(0);
        let mut page = Vec::with_capacity(self.size.min(room));
        page.extend(self.inner.by_ref().take(self.size));
        if page.is_empty() { None } else { Some(page) }
    }

    fn nth(&mut self, n: usize) -> Option<Vec<I::Item>> {
        let skip = n.checked_mul(self.size)?;
        if skip > 0 {
            self.inner.nth(skip - 1)?;
        }
        self.next()
    }
}
