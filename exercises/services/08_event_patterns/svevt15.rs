// Проекцию ленты решили пересобрать с начала журнала: перечитать все события и применить заново. Результат не совпал
// с живой проекцией: у постов другое время, а порядок постов с одинаковым временем меняется от запуска к запуску.
// Цель: проекция зависит только от событий. Пересборка из того же журнала всегда даёт тот же результат, хоть через
// секунду, хоть через год. Время поста берётся из события; посты с равным временем идут по идентификатору по возрастанию (в ленте из прошлой задачи было по убыванию: здесь порядок
// пересборки, не показа). Всё, что после правки стало лишним, удали.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

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
pub struct Post {
    pub author: String,
    pub text: String,
    pub created_at: u64,
}

#[derive(Default)]
pub struct Timeline {
    posts: HashMap<String, Post>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Timeline {
    pub fn new() -> Timeline {
        Timeline::default()
    }

    pub fn apply(&mut self, event: &Event) {
        let text = |key: &str| event.payload.get(key).and_then(Value::as_str).map(str::to_string);
        match event.kind.as_str() {
            "post.created" => {
                if let (Some(id), Some(author), Some(body)) = (text("post_id"), text("author_id"), text("text")) {
                    self.posts.insert(id, Post { author, text: body, created_at: now_ms() });
                }
            }
            "post.deleted" => {
                if let Some(id) = text("post_id") {
                    self.posts.remove(&id);
                }
            }
            _ => {}
        }
    }

    pub fn latest(&self, limit: usize) -> Vec<String> {
        let mut ids: Vec<(&String, &Post)> = self.posts.iter().collect();
        ids.sort_by_key(|(_, post)| std::cmp::Reverse(post.created_at));
        ids.into_iter().take(limit).map(|(id, _)| id.clone()).collect()
    }

    pub fn snapshot(&self) -> Vec<(String, Post)> {
        let mut all: Vec<(String, Post)> = self.posts.iter().map(|(id, post)| (id.clone(), post.clone())).collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all
    }

    pub fn rebuild(journal: &[Event]) -> Timeline {
        let mut timeline = Timeline::new();
        for event in journal {
            timeline.apply(event);
        }
        timeline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn created(id: &str, at: u64) -> Event {
        Event {
            event_id: format!("e-{id}"),
            kind: "post.created".into(),
            schema: 1,
            version: 1,
            occurred_at: at,
            aggregate_id: id.into(),
            payload: json!({"post_id": id, "author_id": "alice", "text": format!("пост {id}")}),
        }
    }

    fn deleted(id: &str, at: u64) -> Event {
        Event { event_id: format!("d-{id}"), kind: "post.deleted".into(), schema: 1, version: 2, occurred_at: at, aggregate_id: id.into(), payload: json!({"post_id": id}) }
    }

    fn journal() -> Vec<Event> {
        let mut events: Vec<Event> = (0..12).map(|i| created(&format!("p{i:02}"), 1_000 + (i / 3) as u64)).collect();
        events.push(deleted("p04", 2_000));
        events
    }

    #[test]
    fn the_time_of_a_post_is_the_time_of_its_event() {
        let timeline = Timeline::rebuild(&[created("p1", 1_234)]);
        assert_eq!(timeline.snapshot()[0].1.created_at, 1_234, "время поста пришло в событии, а не из часов сервера в момент пересборки");
    }

    #[test]
    fn a_rebuild_equals_the_live_projection_even_when_time_has_passed() {
        let mut live = Timeline::new();
        for event in journal() {
            live.apply(&event);
            std::thread::sleep(Duration::from_millis(3));
        }
        let rebuilt = Timeline::rebuild(&journal());
        assert_eq!(rebuilt.snapshot(), live.snapshot(), "пересборка должна давать то же состояние, что и живая работа");
    }

    #[test]
    fn posts_with_equal_times_always_come_in_the_same_order() {
        let first = Timeline::rebuild(&journal()).latest(20);
        for round in 0..30 {
            assert_eq!(Timeline::rebuild(&journal()).latest(20), first, "раунд {round}: порядок одинаковых по времени постов поплыл (HashMap перебирается в случайном порядке)");
        }
        assert_eq!(first[..3], ["p09", "p10", "p11"], "самые новые по времени, а внутри равного времени по идентификатору по возрастанию");
        assert!(!first.contains(&"p04".to_string()), "удалённый пост в ленте не появляется");
    }

    #[test]
    fn the_limit_cuts_the_tail() {
        assert_eq!(Timeline::rebuild(&journal()).latest(4), vec!["p09", "p10", "p11", "p06"], "первые четыре после сортировки");
    }
}
