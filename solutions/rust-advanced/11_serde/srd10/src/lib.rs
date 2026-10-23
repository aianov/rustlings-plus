use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Post {
    pub id: u64,
    pub title: String,
}

pub fn author_name(doc: &Value) -> Option<&str> {
    doc["post"]["author"]["name"].as_str()
}

pub fn add_like(doc: &mut Value) {
    let likes = doc["stats"]["likes"].as_u64().unwrap_or(0);
    doc["stats"]["likes"] = Value::from(likes + 1);
}

pub fn notice(user: &str, count: u32) -> Value {
    serde_json::json!({ "type": "notice", "user": user, "count": count, "tags": [] })
}

pub fn to_post(doc: Value) -> Result<Post, serde_json::Error> {
    serde_json::from_value(doc)
}

pub fn from_post(post: &Post) -> Value {
    serde_json::to_value(post).expect("Post всегда превращается в Value")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_nested_author() {
        let doc = json!({"post": {"author": {"name": "ira", "id": 1}}});
        assert_eq!(author_name(&doc), Some("ira"));
    }

    #[test]
    fn missing_paths_are_none_not_panics() {
        assert_eq!(author_name(&json!({})), None);
        assert_eq!(author_name(&json!({"post": {}})), None);
        assert_eq!(author_name(&json!({"post": {"author": {"name": 5}}})), None, "имя не строка");
        assert_eq!(author_name(&json!("just a string")), None);
        assert_eq!(author_name(&json!([1, 2])), None);
    }

    #[test]
    fn likes_are_counted_even_when_stats_are_missing() {
        let mut doc = json!({"post": 1});
        add_like(&mut doc);
        assert_eq!(doc, json!({"post": 1, "stats": {"likes": 1}}), "недостающие уровни надо создать");
        add_like(&mut doc);
        assert_eq!(doc["stats"]["likes"], json!(2));
        let mut rich = json!({"stats": {"likes": 41, "views": 10}, "keep": true});
        add_like(&mut rich);
        assert_eq!(rich, json!({"stats": {"likes": 42, "views": 10}, "keep": true}), "остальное не трогаем");
    }

    #[test]
    fn notice_has_a_fixed_shape() {
        assert_eq!(notice("ira", 3), json!({"type": "notice", "user": "ira", "count": 3, "tags": []}));
    }

    #[test]
    fn typed_and_untyped_convert_both_ways() {
        let post = Post { id: 5, title: "hi".into() };
        let value = from_post(&post);
        assert_eq!(value, json!({"id": 5, "title": "hi"}));
        assert_eq!(to_post(value).unwrap(), post);
        let err = to_post(json!({"id": 5})).unwrap_err();
        assert!(err.to_string().contains("missing field `title`"), "{err}");
    }
}
