use events::{Event, UpcastError, upcast};
use serde_json::json;

fn profile(schema: u32, payload: serde_json::Value) -> Event {
    Event::new("e1".into(), "user.profile_changed", schema, 2, 50, "u1", payload)
}

#[test]
fn the_old_name_becomes_first_and_last_name() {
    let up = upcast(profile(1, json!({"name": "Ann Marie Lee", "locale": "ru"}))).expect("schema 1 поднимается");
    assert_eq!(up.schema, 2, "после апкаста схема вторая");
    assert_eq!(up.payload, json!({"first_name": "Ann", "last_name": "Marie Lee", "locale": "ru"}), "имя режется по первому пробелу, чужие поля остаются");
    assert_eq!((up.event_id.as_str(), up.version, up.occurred_at, up.aggregate_id.as_str()), ("e1", 2, 50, "u1"), "конверт не меняется");
}

#[test]
fn a_single_word_name_has_an_empty_last_name() {
    let up = upcast(profile(1, json!({"name": "Cher"}))).unwrap();
    assert_eq!(up.payload, json!({"first_name": "Cher", "last_name": ""}), "одно слово: фамилия пустая");
}

#[test]
fn current_and_foreign_events_pass_through() {
    let fresh = profile(2, json!({"first_name": "Ann", "last_name": "Lee"}));
    assert_eq!(upcast(fresh.clone()), Ok(fresh), "вторая схема не меняется");
    let other = Event::new("e2".into(), "post.created", 1, 1, 5, "p1", json!({"text": "x"}));
    assert_eq!(upcast(other.clone()), Ok(other), "чужие типы не трогаем");
}

#[test]
fn unknown_schemas_and_bad_payloads_are_errors() {
    assert_eq!(upcast(profile(3, json!({}))), Err(UpcastError::UnknownSchema(3)), "схема из будущего");
    assert_eq!(upcast(profile(1, json!({"name": "  "}))), Err(UpcastError::BadPayload("name")), "пустое имя");
    assert_eq!(upcast(profile(1, json!({}))), Err(UpcastError::BadPayload("name")), "нет имени");
}
