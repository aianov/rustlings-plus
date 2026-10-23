
use std::collections::{HashMap, HashSet, VecDeque};

fn main() {}

pub struct Timelines {
    cap: usize,
    threshold: usize,
    followers: HashMap<u64, Vec<u64>>,
    following: HashMap<u64, Vec<u64>>,
    lines: HashMap<u64, VecDeque<u64>>,
    celebrity_posts: HashMap<u64, VecDeque<u64>>,
    writes: u64,
}

impl Timelines {
    pub fn new(cap: usize, threshold: usize) -> Timelines {
        Timelines {
            cap,
            threshold,
            followers: HashMap::new(),
            following: HashMap::new(),
            lines: HashMap::new(),
            celebrity_posts: HashMap::new(),
            writes: 0,
        }
    }

    pub fn follow(&mut self, user: u64, author: u64) {
        let list = self.followers.entry(author).or_default();
        if !list.contains(&user) {
            list.push(user);
            self.following.entry(user).or_default().push(author);
        }
    }

    /// Сколько раз всего дописывали в личные ленты.
    pub fn writes(&self) -> u64 {
        self.writes
    }

    pub fn publish(&mut self, author: u64, post: u64) {
        if self.followers.get(&author).map_or(0, Vec::len) >= self.threshold {
            let store = self.celebrity_posts.entry(author).or_default();
            if !store.contains(&post) {
                store.push_front(post);
                store.truncate(self.cap);
            }
            return;
        }
        for &user in self.followers.get(&author).map(Vec::as_slice).unwrap_or(&[]) {
            let line = self.lines.entry(user).or_default();
            if line.contains(&post) {
                continue;
            }
            line.push_front(post);
            line.truncate(self.cap);
            self.writes += 1;
        }
    }

    pub fn timeline(&self, user: u64, limit: usize) -> Vec<u64> {
        let mut all: HashSet<u64> = self.lines.get(&user).map(|l| l.iter().copied().collect()).unwrap_or_default();
        for author in self.following.get(&user).map(Vec::as_slice).unwrap_or(&[]) {
            if let Some(posts) = self.celebrity_posts.get(author) {
                all.extend(posts.iter().copied());
            }
        }
        let mut merged: Vec<u64> = all.into_iter().collect();
        merged.sort_unstable_by(|a, b| b.cmp(a));
        merged.truncate(limit);
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> Timelines {
        let mut t = Timelines::new(5, 3);
        t.follow(1, 10);
        t.follow(2, 10);
        for fan in 1..=5 {
            t.follow(fan, 99);
        }
        t.follow(1, 98);
        t.follow(1, 97);
        t.follow(2, 97);
        t.follow(3, 97);
        t
    }

    #[test]
    fn an_ordinary_author_is_spread_on_write() {
        let mut t = world();
        t.publish(10, 50);
        assert_eq!(t.writes(), 2);
        assert_eq!(t.timeline(1, 10), vec![50]);
    }

    #[test]
    fn a_celebrity_post_is_not_spread_but_is_visible() {
        let mut t = world();
        t.publish(99, 60);
        assert_eq!(t.writes(), 0, "пост знаменитости (5 подписчиков, порог 3) записан в личные ленты: {} записей", t.writes());
        for fan in 1..=5 {
            assert_eq!(t.timeline(fan, 10), vec![60], "подписчик {fan} должен видеть пост сразу");
        }
        assert!(t.timeline(77, 10).is_empty(), "не подписанный не видит");
    }

    #[test]
    fn reading_merges_both_sources_by_post_number() {
        let mut t = world();
        t.publish(10, 50);
        t.publish(99, 60);
        t.publish(10, 70);
        t.publish(99, 80);
        assert_eq!(t.timeline(1, 10), vec![80, 70, 60, 50]);
        assert_eq!(t.timeline(1, 3), vec![80, 70, 60], "limit обрезает уже слитую ленту");
        assert_eq!(t.timeline(2, 10), vec![80, 70, 60, 50]);
    }

    #[test]
    fn several_celebrities_are_merged_and_there_are_no_duplicates() {
        let mut t = world();
        t.publish(99, 60);
        t.publish(97, 61);
        t.publish(98, 62);
        assert_eq!(t.timeline(1, 10), vec![62, 61, 60], "пользователь 1 подписан на трёх; 98 с одним подписчиком раскладывается, остальные подмешиваются");
        t.publish(99, 60);
        assert_eq!(t.timeline(1, 10), vec![62, 61, 60], "повтор события ничего не меняет");
    }

    #[test]
    fn an_author_who_becomes_a_celebrity_keeps_old_posts_visible() {
        let mut t = Timelines::new(5, 3);
        t.follow(1, 10);
        t.follow(2, 10);
        t.publish(10, 5);
        t.follow(3, 10);
        t.publish(10, 6);
        assert_eq!(t.timeline(1, 10), vec![6, 5], "после перехода порога старые посты из личных лент и новые из общего хранилища читаются вместе");
        assert_eq!(t.writes(), 2, "второй пост уже не раскладывался");
    }

    #[test]
    fn the_celebrity_store_is_capped() {
        let mut t = world();
        for post in 100..120 {
            t.publish(99, post);
        }
        assert_eq!(t.timeline(1, 100), vec![119, 118, 117, 116, 115], "хранилище знаменитости не растёт без предела");
    }
}
