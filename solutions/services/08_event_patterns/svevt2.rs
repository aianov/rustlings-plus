use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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

#[derive(Debug, PartialEq)]
pub enum UpcastError {
    UnknownSchema(u32),
    BadPayload(&'static str),
    WrongKind,
}

#[derive(Debug, PartialEq)]
pub struct ProfileChanged {
    pub user_id: String,
    pub first_name: String,
    pub last_name: String,
    pub avatar: Option<String>,
}

pub fn upcast(event: Event) -> Result<Event, UpcastError> {
    let mut event = event;
    if event.kind != "user.profile_changed" {
        return Ok(event);
    }
    if event.schema == 1 {
        let name = event.payload.get("name").and_then(Value::as_str).ok_or(UpcastError::BadPayload("name"))?.trim().to_string();
        if name.is_empty() {
            return Err(UpcastError::BadPayload("name"));
        }
        let (first, last) = match name.split_once(char::is_whitespace) {
            Some((first, last)) => (first.to_string(), last.trim_start().to_string()),
            None => (name, String::new()),
        };
        let object = event.payload.as_object_mut().ok_or(UpcastError::BadPayload("payload"))?;
        object.remove("name");
        object.insert("first_name".to_string(), json!(first));
        object.insert("last_name".to_string(), json!(last));
        event.schema = 2;
    }
    match event.schema {
        2 => Ok(event),
        other => Err(UpcastError::UnknownSchema(other)),
    }
}

impl ProfileChanged {
    pub fn from_event(event: &Event) -> Result<ProfileChanged, UpcastError> {
        if event.kind != "user.profile_changed" {
            return Err(UpcastError::WrongKind);
        }
        let event = upcast(event.clone())?;
        let text = |key: &'static str| event.payload.get(key).and_then(Value::as_str).map(str::to_string).ok_or(UpcastError::BadPayload(key));
        Ok(ProfileChanged {
            user_id: event.aggregate_id.clone(),
            first_name: text("first_name")?,
            last_name: text("last_name")?,
            avatar: event.payload.get("avatar").and_then(Value::as_str).map(str::to_string),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(schema: u32, payload: Value) -> Event {
        Event { event_id: "e7".into(), kind: "user.profile_changed".into(), schema, version: 4, occurred_at: 99, aggregate_id: "u1".into(), payload }
    }

    #[test]
    fn the_old_name_is_split_into_two_fields() {
        let up = upcast(profile(1, json!({"name": "Ann Lee"}))).expect("schema 1 поднимается");
        assert_eq!(up.schema, 2, "после апкаста схема вторая");
        assert_eq!(up.payload, json!({"first_name": "Ann", "last_name": "Lee"}), "поле name заменено двумя новыми");
    }

    #[test]
    fn the_name_is_cut_at_the_first_space_only() {
        let up = upcast(profile(1, json!({"name": "  Ann Marie  Lee "}))).unwrap();
        assert_eq!(up.payload["first_name"], "Ann", "имя до первого пробела, лишние пробелы по краям не считаются");
        assert_eq!(up.payload["last_name"], "Marie  Lee", "всё остальное это фамилия");
        let single = upcast(profile(1, json!({"name": "Cher"}))).unwrap();
        assert_eq!((single.payload["first_name"].as_str(), single.payload["last_name"].as_str()), (Some("Cher"), Some("")), "одно слово: фамилия пустая");
    }

    #[test]
    fn everything_but_the_payload_shape_is_kept() {
        let old = profile(1, json!({"name": "Ann Lee", "avatar": "a.png", "locale": "ru"}));
        let up = upcast(old.clone()).unwrap();
        assert_eq!((&up.event_id, up.version, up.occurred_at, &up.aggregate_id), (&old.event_id, 4, 99, &old.aggregate_id), "конверт не меняется");
        assert_eq!(up.payload["avatar"], "a.png", "чужие поля payload сохраняются");
        assert_eq!(up.payload["locale"], "ru", "и неизвестные тоже");
    }

    #[test]
    fn a_current_event_and_foreign_kinds_pass_through_unchanged() {
        let fresh = profile(2, json!({"first_name": "Ann", "last_name": "Lee"}));
        assert_eq!(upcast(fresh.clone()), Ok(fresh), "второй схеме апкаст не нужен");
        let other = Event { kind: "post.created".into(), ..profile(1, json!({"name": "x"})) };
        assert_eq!(upcast(other.clone()), Ok(other), "чужие типы не трогаем, даже если схема 1");
    }

    #[test]
    fn unknown_schemas_and_broken_payloads_are_errors() {
        assert_eq!(upcast(profile(3, json!({}))), Err(UpcastError::UnknownSchema(3)), "из будущего не гадаем");
        assert_eq!(upcast(profile(0, json!({}))), Err(UpcastError::UnknownSchema(0)), "нулевой схемы не было");
        assert_eq!(upcast(profile(1, json!({"name": "   "}))), Err(UpcastError::BadPayload("name")), "пустое имя");
        assert_eq!(upcast(profile(1, json!({"nick": "x"}))), Err(UpcastError::BadPayload("name")), "нет имени");
        assert_eq!(upcast(profile(1, json!({"name": 42}))), Err(UpcastError::BadPayload("name")), "имя не строка");
    }

    #[test]
    fn the_reader_accepts_both_schemas_and_ignores_extras() {
        let old = ProfileChanged::from_event(&profile(1, json!({"name": "Ann Lee", "locale": "ru"}))).unwrap();
        let new = ProfileChanged::from_event(&profile(2, json!({"first_name": "Ann", "last_name": "Lee", "avatar": "a.png", "badge": 5}))).unwrap();
        assert_eq!((old.first_name.as_str(), old.last_name.as_str(), old.avatar.as_deref()), ("Ann", "Lee", None), "старое событие читается как новое");
        assert_eq!(new.user_id, "u1", "пользователь это агрегат");
        assert_eq!(new.avatar.as_deref(), Some("a.png"), "необязательное поле читается, если оно есть");
    }

    #[test]
    fn the_reader_refuses_foreign_kinds_and_incomplete_new_events() {
        let post = Event { kind: "post.created".into(), ..profile(2, json!({})) };
        assert_eq!(ProfileChanged::from_event(&post), Err(UpcastError::WrongKind), "чужой тип");
        assert_eq!(ProfileChanged::from_event(&profile(2, json!({"first_name": "Ann"}))), Err(UpcastError::BadPayload("last_name")), "обязательное поле не придумываем");
    }
}
