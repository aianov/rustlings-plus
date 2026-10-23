
use std::collections::{HashMap, VecDeque};

fn main() {}

pub struct Timelines {
    cap: usize,
    followers: HashMap<u64, Vec<u64>>,
    lines: HashMap<u64, VecDeque<u64>>,
    writes: u64,
}

impl Timelines {
    pub fn new(cap: usize) -> Timelines {
        Timelines { cap, followers: HashMap::new(), lines: HashMap::new(), writes: 0 }
    }

    pub fn follow(&mut self, user: u64, author: u64) {
        let list = self.followers.entry(author).or_default();
        if !list.contains(&user) {
            list.push(user);
        }
    }

    /// Сколько раз всего дописывали в ленты.
    pub fn writes(&self) -> u64 {
        self.writes
    }

    /// Раскладывает пост по лентам подписчиков, возвращает, в сколько лент он действительно записан.
    pub fn publish(&mut self, author: u64, post: u64) -> usize {
        let mut stored = 0;
        for &user in self.followers.get(&author).map(Vec::as_slice).unwrap_or(&[]) {
            let line = self.lines.entry(user).or_default();
            if line.contains(&post) {
                continue;
            }
            line.push_front(post);
            line.truncate(self.cap);
            self.writes += 1;
            stored += 1;
        }
        stored
    }

    /// Первые `limit` номеров ленты, от новых к старым.
    pub fn timeline(&self, user: u64, limit: usize) -> Vec<u64> {
        self.lines.get(&user).map(|l| l.iter().take(limit).copied().collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> Timelines {
        let mut t = Timelines::new(3);
        t.follow(1, 100);
        t.follow(2, 100);
        t.follow(2, 200);
        t
    }

    #[test]
    fn followers_get_the_post_and_others_do_not() {
        let mut t = world();
        assert_eq!(t.publish(100, 10), 2);
        assert_eq!(t.timeline(1, 10), vec![10]);
        assert_eq!(t.timeline(2, 10), vec![10]);
        assert!(t.timeline(3, 10).is_empty(), "пользователь 3 ни на кого не подписан");
    }

    #[test]
    fn the_timeline_is_newest_first_and_merges_authors() {
        let mut t = world();
        t.publish(100, 10);
        t.publish(200, 11);
        t.publish(100, 12);
        assert_eq!(t.timeline(2, 10), vec![12, 11, 10]);
        assert_eq!(t.timeline(1, 10), vec![12, 10]);
        assert_eq!(t.timeline(2, 2), vec![12, 11], "limit обрезает чтение");
    }

    #[test]
    fn the_line_keeps_only_cap_newest() {
        let mut t = world();
        for post in 1..=6 {
            t.publish(100, post);
        }
        assert_eq!(t.timeline(1, 10), vec![6, 5, 4], "в ленте не больше cap = 3 самых новых");
    }

    #[test]
    fn a_new_follower_does_not_get_old_posts() {
        let mut t = world();
        t.publish(100, 10);
        t.follow(5, 100);
        assert!(t.timeline(5, 10).is_empty(), "раскладка делается при публикации, а не при подписке");
        t.publish(100, 11);
        assert_eq!(t.timeline(5, 10), vec![11]);
    }

    #[test]
    fn republishing_the_same_post_changes_nothing() {
        let mut t = world();
        t.publish(100, 10);
        let writes = t.writes();
        assert_eq!(t.publish(100, 10), 0, "событие доставлено дважды: записывать нечего");
        assert_eq!(t.timeline(1, 10), vec![10], "дубль в ленте");
        assert_eq!(t.writes(), writes);
    }

    #[test]
    fn following_twice_is_one_subscription() {
        let mut t = world();
        t.follow(1, 100);
        assert_eq!(t.publish(100, 10), 2);
    }
}
