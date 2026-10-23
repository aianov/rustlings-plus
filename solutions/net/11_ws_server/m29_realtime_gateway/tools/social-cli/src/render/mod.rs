use std::collections::HashMap;

use serde_json::Value;

#[derive(Default)]
pub struct Names {
    by_id: HashMap<String, String>,
    by_name: HashMap<String, String>,
}

impl Names {
    pub fn remember(&mut self, id: &str, name: &str) {
        self.by_id.insert(id.to_string(), name.to_string());
        self.by_name.insert(name.to_lowercase(), id.to_string());
    }

    pub fn remember_user(&mut self, user: &Value) {
        if let (Some(id), Some(name)) = (text(user, "id"), text(user, "username")) {
            self.remember(id, name);
        }
    }

    pub fn id_of(&self, name: &str) -> Option<String> {
        self.by_name.get(&name.to_lowercase()).cloned()
    }

    pub fn name(&self, id: &str) -> String {
        match self.by_id.get(id) {
            Some(name) => format!("@{name}"),
            None => id.to_string(),
        }
    }

    pub fn post(&mut self, post: &Value) -> String {
        let id = text(post, "id").unwrap_or_default();
        let author = text(post, "author").unwrap_or_default();
        if let Some(name) = text(post, "author_name").filter(|name| !name.is_empty()) {
            self.remember(author, name);
        }
        let number = id.strip_prefix("p_").unwrap_or(id);
        let likes = post.get("likes").and_then(Value::as_u64).unwrap_or(0);
        format!(
            "{} [{number}] {} (лайков: {likes})",
            self.name(author),
            text(post, "text").unwrap_or_default()
        )
    }

    pub fn message(&self, message: &Value) -> String {
        let from = text(message, "from").unwrap_or_default();
        format!(
            "{}: {}",
            self.name(from),
            text(message, "text").unwrap_or_default()
        )
    }

    pub fn push(&self, push: &Value) -> String {
        let data = push.get("data").unwrap_or(&Value::Null);
        let who = |field| self.name(text(data, field).unwrap_or_default());
        let post = text(data, "post")
            .map(|post| post.strip_prefix("p_").unwrap_or(post).to_string())
            .unwrap_or_default();
        match text(push, "event").unwrap_or_default() {
            "followed" => format!("* на тебя подписался {}", who("by")),
            "liked" => format!("* {} лайкнул твой пост [{post}]", who("by")),
            "new_post" => format!("* новый пост [{post}] от {}", who("author")),
            "message" => format!("* новое сообщение от {}", who("from")),
            other => format!("* событие {other}"),
        }
    }
}

pub fn text<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}
