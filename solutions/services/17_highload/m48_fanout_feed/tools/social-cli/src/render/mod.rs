use std::collections::HashMap;

use serde_json::Value;

#[derive(Default)]
pub struct Names {
    by_id: HashMap<String, String>,
    by_name: HashMap<String, String>,
    aliases: HashMap<String, u64>,
    reals: HashMap<u64, String>,
}

impl Names {
    pub fn alias(&mut self, post: &str) -> u64 {
        if let Some(alias) = self.aliases.get(post) {
            return *alias;
        }
        let alias = self.aliases.len() as u64 + 1;
        self.aliases.insert(post.to_string(), alias);
        self.reals.insert(alias, post.to_string());
        alias
    }

    pub fn real(&self, post: &str) -> String {
        let number = post.strip_prefix("p_").unwrap_or(post);
        number
            .parse::<u64>()
            .ok()
            .and_then(|alias| self.reals.get(&alias).cloned())
            .unwrap_or_else(|| post.to_string())
    }

    pub fn remember(&mut self, id: &str, name: &str) {
        self.by_id.insert(id.to_string(), name.to_string());
        self.by_name.insert(name.to_lowercase(), id.to_string());
    }

    pub fn remember_user(&mut self, user: &Value) {
        if let (Some(id), Some(name)) = (text(user, "id"), text(user, "username")) {
            self.remember(id, name);
        }
    }

    pub fn knows(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
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
        let number = self.alias(id);
        let likes = post.get("likes").and_then(Value::as_u64).unwrap_or(0);
        let mut line = format!(
            "{} [{number}] {} (лайков: {likes})",
            self.name(author),
            text(post, "text").unwrap_or_default()
        );
        let words = |field: &str, key: Option<&str>| -> Vec<String> {
            let items = post.get(field).and_then(Value::as_array);
            items
                .into_iter()
                .flatten()
                .filter_map(|item| match key {
                    Some(key) => text(item, key).map(str::to_string),
                    None => item.as_str().map(str::to_string),
                })
                .collect()
        };
        let tags = words("tags", None);
        if !tags.is_empty() {
            line.push_str(&format!(" #{}", tags.join(" #")));
        }
        for url in words("links", Some("url")) {
            line.push_str(&format!(" {url}"));
        }
        line
    }

    pub fn message(&self, message: &Value) -> String {
        let from = text(message, "from").unwrap_or_default();
        format!(
            "{}: {}",
            self.name(from),
            text(message, "text").unwrap_or_default()
        )
    }

    pub fn notice(&mut self, notice: &Value) -> String {
        let post = text(notice, "post")
            .map(|post| self.alias(post).to_string())
            .unwrap_or_default();
        let actor = self.name(text(notice, "actor").unwrap_or_default());
        let mark = match notice.get("read").and_then(Value::as_bool) {
            Some(true) => " ",
            _ => "•",
        };
        let what = match text(notice, "kind").unwrap_or_default() {
            "followed" => format!("{actor} подписался на тебя"),
            "liked" => format!("{actor} лайкнул твой пост [{post}]"),
            "commented" => format!("{actor} прокомментировал твой пост [{post}]"),
            "new_post" => format!("новый пост [{post}] от {actor}"),
            "digest" => {
                let count = notice.get("count").and_then(Value::as_u64).unwrap_or(0);
                format!("сводка: непрочитанных было {count}")
            }
            other => format!("{other} от {actor}"),
        };
        format!("  {mark} {what}")
    }

    pub fn push(&mut self, push: &Value) -> String {
        let data = push.get("data").unwrap_or(&Value::Null);
        if text(push, "event") == Some("feed_post") {
            let line = self.post(data.get("post").unwrap_or(&Value::Null));
            return match data.get("live").and_then(Value::as_bool) {
                Some(true) => format!("* новое в ленте: {line}"),
                _ => line,
            };
        }
        let post = text(data, "post")
            .map(|post| self.alias(post).to_string())
            .unwrap_or_default();
        let who = |field| self.name(text(data, field).unwrap_or_default());
        match text(push, "event").unwrap_or_default() {
            "followed" => format!("* на тебя подписался {}", who("by")),
            "liked" => format!("* {} лайкнул твой пост [{post}]", who("by")),
            "commented" => format!("* {} прокомментировал твой пост [{post}]", who("by")),
            "new_post" => format!("* новый пост [{post}] от {}", who("author")),
            "message" => format!("* {}", self.message(data)),
            "typing" => format!("* {} печатает…", who("from")),
            "digest" => {
                let unread = data.get("unread").and_then(Value::as_u64).unwrap_or(0);
                format!("* сводка: непрочитанных {unread}, загляни в bell")
            }
            "feed_caught_up" => String::from("* дальше только новые посты"),
            other => format!("* событие {other}"),
        }
    }
}

pub fn text<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}
