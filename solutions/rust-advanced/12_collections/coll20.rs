use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u64,
    pub author: u64,
    pub ts: u64,
    pub text: String,
}

#[derive(Default)]
pub struct Store {
    posts: HashMap<u64, Post>,
    by_author: HashMap<u64, BTreeSet<(u64, u64)>>,
}

impl Store {
    pub fn get(&self, id: u64) -> Option<&Post> {
        self.posts.get(&id)
    }

    pub fn authors_indexed(&self) -> usize {
        self.by_author.len()
    }

    pub fn add(&mut self, post: Post) {
        let id = post.id;
        self.unindex(id);
        self.by_author.entry(post.author).or_default().insert((post.ts, id));
        self.posts.insert(id, post);
    }

    pub fn reassign(&mut self, id: u64, new_author: u64) -> bool {
        let Some(mut post) = self.posts.get(&id).cloned() else {
            return false;
        };
        post.author = new_author;
        self.add(post);
        true
    }

    pub fn author_feed(&self, author: u64) -> Vec<u64> {
        self.by_author.get(&author).map(|set| set.iter().rev().map(|(_, id)| *id).collect()).unwrap_or_default()
    }

    fn unindex(&mut self, id: u64) {
        let Some(old) = self.posts.get(&id) else {
            return;
        };
        let (author, ts) = (old.author, old.ts);
        if let Some(set) = self.by_author.get_mut(&author) {
            set.remove(&(ts, id));
            if set.is_empty() {
                self.by_author.remove(&author);
            }
        }
    }
}

fn main() {
    let mut store = Store::default();
    store.add(Post { id: 1, author: 7, ts: 100, text: "hi".to_string() });
    println!("{:?}", store.author_feed(7));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(id: u64, author: u64, ts: u64) -> Post {
        Post { id, author, ts, text: format!("post {id}") }
    }

    #[test]
    fn the_feed_is_newest_first() {
        let mut s = Store::default();
        s.add(post(1, 7, 100));
        s.add(post(2, 7, 300));
        s.add(post(3, 7, 200));
        s.add(post(4, 8, 999));
        s.add(post(5, 7, 300));
        assert_eq!(s.author_feed(7), vec![5, 2, 3, 1]);
        assert_eq!(s.author_feed(8), vec![4]);
        assert!(s.author_feed(9).is_empty());
    }

    #[test]
    fn replacing_a_post_drops_its_old_index_entry() {
        let mut s = Store::default();
        s.add(post(1, 7, 100));
        s.add(post(1, 8, 500));
        assert!(s.author_feed(7).is_empty(), "пост 1 переехал к автору 8, у автора 7 его быть не должно");
        assert_eq!(s.author_feed(8), vec![1]);
        s.add(post(1, 8, 600));
        assert_eq!(s.author_feed(8), vec![1], "пост заменён, а не добавлен второй раз");
        assert_eq!(s.get(1).map(|p| p.ts), Some(600));
    }

    #[test]
    fn reassigning_moves_the_index_entry() {
        let mut s = Store::default();
        s.add(post(1, 7, 100));
        s.add(post(2, 7, 200));
        assert!(s.reassign(1, 9));
        assert_eq!(s.author_feed(7), vec![2]);
        assert_eq!(s.author_feed(9), vec![1]);
        assert_eq!(s.get(1).map(|p| p.author), Some(9));
        assert!(!s.reassign(404, 9), "несуществующий пост передать нельзя");
    }

    #[test]
    fn empty_authors_leave_no_trace() {
        let mut s = Store::default();
        s.add(post(1, 7, 100));
        s.add(post(2, 8, 100));
        assert_eq!(s.authors_indexed(), 2);
        s.reassign(1, 8);
        assert_eq!(s.authors_indexed(), 1, "у автора 7 не осталось постов: пустую запись надо убрать");
    }
}
