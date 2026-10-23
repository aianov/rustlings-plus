use std::collections::HashMap;
use std::collections::hash_map::Entry;

#[derive(Default)]
pub struct Likes {
    counts: HashMap<u64, u32>,
}

impl Likes {
    pub fn new() -> Likes {
        Likes::default()
    }

    pub fn count(&self, post: u64) -> u32 {
        self.counts.get(&post).copied().unwrap_or(0)
    }

    pub fn tracked(&self) -> usize {
        self.counts.len()
    }

    pub fn like(&mut self, post: u64) -> u32 {
        let n = self.counts.entry(post).or_insert(0);
        *n += 1;
        *n
    }

    pub fn unlike(&mut self, post: u64) -> u32 {
        match self.counts.entry(post) {
            Entry::Occupied(mut e) => {
                if *e.get() <= 1 {
                    e.remove();
                    0
                } else {
                    *e.get_mut() -= 1;
                    *e.get()
                }
            }
            Entry::Vacant(_) => 0,
        }
    }
}

pub fn title_or_insert_with<F: FnOnce() -> String>(titles: &mut HashMap<u64, String>, post: u64, make: F) -> &String {
    titles.entry(post).or_insert_with(make)
}

fn main() {
    let mut likes = Likes::new();
    likes.like(7);
    println!("{}", likes.count(7));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn likes_are_counted_per_post() {
        let mut l = Likes::new();
        assert_eq!(l.like(1), 1);
        assert_eq!(l.like(1), 2);
        assert_eq!(l.like(2), 1);
        assert_eq!((l.count(1), l.count(2), l.count(3)), (2, 1, 0));
    }

    #[test]
    fn the_last_unlike_removes_the_entry() {
        let mut l = Likes::new();
        l.like(5);
        l.like(5);
        assert_eq!(l.unlike(5), 1);
        assert_eq!(l.tracked(), 1);
        assert_eq!(l.unlike(5), 0);
        assert_eq!(l.tracked(), 0, "на нуле запись должна пропасть, иначе карта копит мёртвые посты");
    }

    #[test]
    fn unliking_an_unknown_post_creates_nothing() {
        let mut l = Likes::new();
        assert_eq!(l.unlike(99), 0);
        assert_eq!(l.tracked(), 0, "unlike неизвестного поста не должен заводить запись");
    }

    #[test]
    fn the_default_title_is_built_only_when_missing() {
        let built = Cell::new(0);
        let mut titles = HashMap::new();
        let first = title_or_insert_with(&mut titles, 1, || {
            built.set(built.get() + 1);
            "untitled".to_string()
        })
        .clone();
        assert_eq!(first, "untitled");
        titles.insert(2, "hello".to_string());
        let second = title_or_insert_with(&mut titles, 2, || {
            built.set(built.get() + 1);
            "untitled".to_string()
        })
        .clone();
        assert_eq!(second, "hello");
        let again = title_or_insert_with(&mut titles, 1, || {
            built.set(built.get() + 1);
            "other".to_string()
        })
        .clone();
        assert_eq!(again, "untitled");
        assert_eq!(built.get(), 1, "заголовок строится лениво: только когда поста нет, а не на каждый вызов");
    }
}
