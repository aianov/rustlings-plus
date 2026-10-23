// Индекс «хештег -> посты» хранит ключи типа `Tag` (обёртка над строкой, чтобы тег нельзя было спутать с
// произвольным текстом). Запрос приходит как `&str`, и на каждый запрос заводить новый `Tag` значит выделять
// память ради поиска. Для этого есть `Borrow`: карта умеет искать по «заимствованному виду» ключа. Второй
// индекс: маршруты по сегментам пути, ключ `Vec<S>`, а ищут по срезу `&[S]`. `Tag::new` и клонирование
// сегментов тест считает: во время поиска их быть не должно.
// TODO: Допиши `Borrow` для `Tag` и методы поиска.
use std::borrow::Borrow;
use std::cell::Cell;
use std::collections::HashMap;
use std::hash::Hash;

thread_local! {
    static NEW_TAGS: Cell<usize> = const { Cell::new(0) };
}

pub fn tags_built() -> usize {
    NEW_TAGS.with(Cell::get)
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Tag(String);

impl Tag {
    pub fn new(name: &str) -> Tag {
        NEW_TAGS.with(|c| c.set(c.get() + 1));
        Tag(name.to_string())
    }
}

#[derive(Default)]
pub struct TagIndex {
    posts: HashMap<Tag, Vec<u64>>,
}

impl TagIndex {
    pub fn add(&mut self, tag: &str, post: u64) {
        self.posts.entry(Tag::new(tag)).or_default().push(post);
    }

    pub fn posts_with(&self, tag: &str) -> &[u64] {
        todo!("посты тега без создания Tag")
    }

    pub fn knows(&self, tag: &str) -> bool {
        todo!("есть ли такой тег")
    }
}

pub struct Routes<S> {
    map: HashMap<Vec<S>, u32>,
}

impl<S: Hash + Eq> Routes<S> {
    pub fn new(entries: Vec<(Vec<S>, u32)>) -> Routes<S> {
        Routes { map: entries.into_iter().collect() }
    }

    pub fn find(&self, path: &[S]) -> Option<u32> {
        todo!("обработчик по срезу сегментов без копирования")
    }
}

fn main() {
    let mut index = TagIndex::default();
    index.add("rust", 1);
    println!("{:?}", index.posts_with("rust"));
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        static CLONES: Cell<usize> = const { Cell::new(0) };
    }

    #[derive(Debug, PartialEq, Eq, Hash)]
    struct Seg(&'static str);

    impl Clone for Seg {
        fn clone(&self) -> Seg {
            CLONES.with(|c| c.set(c.get() + 1));
            Seg(self.0)
        }
    }

    #[test]
    fn tags_are_found_by_str() {
        let mut index = TagIndex::default();
        index.add("rust", 1);
        index.add("rust", 5);
        index.add("cats", 2);
        assert_eq!(index.posts_with("rust"), &[1, 5]);
        assert_eq!(index.posts_with("cats"), &[2]);
        assert!(index.posts_with("dogs").is_empty());
        assert!(index.knows("cats"));
        assert!(!index.knows("dogs"));
    }

    #[test]
    fn lookups_do_not_allocate_tags() {
        let mut index = TagIndex::default();
        index.add("rust", 1);
        let before = tags_built();
        for _ in 0..10 {
            index.posts_with("rust");
            index.knows("rust");
            index.knows("nothing");
        }
        assert_eq!(tags_built(), before, "для поиска не нужно строить Tag: ищи по &str через Borrow");
    }

    #[test]
    fn routes_are_found_by_slice_without_cloning() {
        let routes = Routes::new(vec![
            (vec![Seg("users"), Seg("me")], 1),
            (vec![Seg("posts")], 2),
            (vec![Seg("posts"), Seg("new")], 3),
        ]);
        let before = CLONES.with(Cell::get);
        assert_eq!(routes.find(&[Seg("posts"), Seg("new")]), Some(3));
        assert_eq!(routes.find(&[Seg("posts")]), Some(2));
        assert_eq!(routes.find(&[Seg("posts"), Seg("old")]), None);
        assert_eq!(routes.find(&[]), None);
        assert_eq!(CLONES.with(Cell::get), before, "искать надо по срезу, а не собирать из него новый Vec");
    }
}
