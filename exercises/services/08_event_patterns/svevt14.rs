// feed-service не ходит в соседние сервисы за данными: он хранит свою копию того, что нужно ленте, и обновляет её
// по событиям. Копия называется проекцией. Цель: Projection::apply разбирает события (user.registered, user.profile_changed
// второй схемы, user.followed, user.unfollowed, post.created) и меняет проекцию; незнакомые события пропускает.
// Projection::feed отдаёт посты тех, на кого viewer подписан, и его собственные: новые первыми (при равном времени
// по идентификатору поста по убыванию), не больше limit. Имя автора берётся на момент чтения, поэтому смена
// имени видна и в старых постах. Если автора проекция ещё не знает, показывается его идентификатор.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

fn main() {}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "one")]
    pub schema: u32,
    pub version: u64,
    pub occurred_at: u64,
    pub aggregate_id: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeedItem {
    pub post_id: String,
    pub author_id: String,
    pub author_name: String,
    pub text: String,
    pub created_at: u64,
}

#[derive(Default)]
pub struct Projection {
    names: HashMap<String, String>,
    follows: HashSet<(String, String)>,
    posts: Vec<(String, String, String, u64)>,
}

fn text(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(str::to_string)
}

impl Projection {
    pub fn new() -> Projection {
        Projection::default()
    }

    pub fn apply(&mut self, event: &Event) {
        let _ = event;
        todo!()
    }

    pub fn feed(&self, viewer: &str, limit: usize) -> Vec<FeedItem> {
        let _ = (viewer, limit);
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str, schema: u32, aggregate: &str, at: u64, payload: Value) -> Event {
        Event { event_id: format!("{kind}-{at}"), kind: kind.into(), schema, version: 1, occurred_at: at, aggregate_id: aggregate.into(), payload }
    }

    fn registered(user: &str, name: &str) -> Event {
        event("user.registered", 1, user, 1, json!({"user_id": user, "name": name}))
    }

    fn followed(a: &str, b: &str) -> Event {
        event("user.followed", 1, a, 2, json!({"follower_id": a, "followee_id": b}))
    }

    fn posted(id: &str, author: &str, at: u64) -> Event {
        event("post.created", 1, id, at, json!({"post_id": id, "author_id": author, "text": format!("текст {id}")}))
    }

    fn built(events: &[Event]) -> Projection {
        let mut projection = Projection::new();
        for e in events {
            projection.apply(e);
        }
        projection
    }

    #[test]
    fn the_feed_shows_followed_authors_and_my_own_posts_newest_first() {
        let projection = built(&[
            registered("alice", "Alice"),
            registered("bob", "Bob"),
            registered("carol", "Carol"),
            followed("alice", "bob"),
            posted("p1", "bob", 10),
            posted("p2", "carol", 20),
            posted("p3", "alice", 30),
            posted("p4", "bob", 40),
        ]);
        let ids: Vec<String> = projection.feed("alice", 10).into_iter().map(|i| i.post_id).collect();
        assert_eq!(ids, vec!["p4", "p3", "p1"], "посты bob и свои, без чужого carol, новые первыми");
        let first = &projection.feed("alice", 10)[0];
        assert_eq!((first.author_name.as_str(), first.text.as_str(), first.created_at), ("Bob", "текст p4", 40), "содержимое элемента ленты");
    }

    #[test]
    fn the_limit_and_equal_times_are_handled() {
        let projection = built(&[followed("alice", "bob"), posted("p1", "bob", 10), posted("p2", "bob", 10), posted("p3", "bob", 10)]);
        let ids: Vec<String> = projection.feed("alice", 2).into_iter().map(|i| i.post_id).collect();
        assert_eq!(ids, vec!["p3", "p2"], "при равном времени по идентификатору по убыванию, и не больше limit");
    }

    #[test]
    fn a_rename_is_visible_in_old_posts() {
        let mut projection = built(&[registered("bob", "Bob"), followed("alice", "bob"), posted("p1", "bob", 10)]);
        assert_eq!(projection.feed("alice", 5)[0].author_name, "Bob", "до переименования");
        projection.apply(&event("user.profile_changed", 2, "bob", 50, json!({"first_name": "Robert", "last_name": "Paulson"})));
        assert_eq!(projection.feed("alice", 5)[0].author_name, "Robert Paulson", "старый пост показывает новое имя");
        projection.apply(&event("user.profile_changed", 2, "bob", 60, json!({"first_name": "Rob", "last_name": ""})));
        assert_eq!(projection.feed("alice", 5)[0].author_name, "Rob", "пустая фамилия не оставляет хвост из пробела");
    }

    #[test]
    fn unfollowing_removes_the_posts_and_unknown_authors_show_their_id() {
        let mut projection = built(&[followed("alice", "ghost"), posted("p1", "ghost", 10)]);
        assert_eq!(projection.feed("alice", 5)[0].author_name, "ghost", "автора ещё нет в проекции: показываем идентификатор");
        projection.apply(&event("user.unfollowed", 1, "alice", 70, json!({"follower_id": "alice", "followee_id": "ghost"})));
        assert!(projection.feed("alice", 5).is_empty(), "после отписки постов этого автора в ленте нет");
    }

    #[test]
    fn unknown_and_broken_events_are_skipped() {
        let projection = built(&[
            event("user.blocked", 1, "x", 5, json!({"anything": true})),
            event("post.created", 1, "p9", 6, json!({"post_id": "p9"})),
            event("user.profile_changed", 1, "bob", 7, json!({"name": "Old Format"})),
            posted("p1", "alice", 8),
        ]);
        let ids: Vec<String> = projection.feed("alice", 5).into_iter().map(|i| i.post_id).collect();
        assert_eq!(ids, vec!["p1"], "мусор не ломает проекцию и не попадает в ленту");
        assert_eq!(projection.feed("alice", 5)[0].author_name, "alice", "событие первой схемы сюда приходит уже поднятым, сам проекция его не читает");
    }
}
