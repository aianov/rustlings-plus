// В каждом сообщении сервера есть общие поля: `id` и время `at`. Хочется описать их один раз в `Meta` и
// вставлять в любой тип, но чтобы на проводе они лежали рядом с остальными полями, а не во вложенном объекте.
// Ещё нужно сохранить поля, о которых этот сервис ничего не знает: он перекладывает их дальше и не имеет права
// терять. Сейчас структуры вкладывают `meta` отдельным объектом, и `Loose` теряет незнакомое.
// TODO: Сделай JSON плоским, как в тестах, и собери незнакомые поля в `extra`.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub id: u64,
    pub at: u64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct PostWire {
    pub meta: Meta,
    pub title: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Liked { post: u64 },
    Followed { user: u64 },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct EventWire {
    pub meta: Meta,
    pub kind: Kind,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Loose {
    pub id: u64,
    pub extra: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn meta_is_flattened_into_the_post() {
        let post = PostWire { meta: Meta { id: 1, at: 5 }, title: "hi".into() };
        assert_eq!(serde_json::to_string(&post).unwrap(), r#"{"id":1,"at":5,"title":"hi"}"#);
        let back: PostWire = serde_json::from_str(r#"{"title":"hi","at":5,"id":1}"#).unwrap();
        assert_eq!(back, post);
    }

    #[test]
    fn event_is_flat_with_a_tag() {
        let event = EventWire { meta: Meta { id: 2, at: 6 }, kind: Kind::Liked { post: 9 } };
        assert_eq!(serde_json::to_string(&event).unwrap(), r#"{"id":2,"at":6,"type":"liked","post":9}"#);
        let back: EventWire = serde_json::from_str(r#"{"id":3,"at":7,"type":"followed","user":4}"#).unwrap();
        assert_eq!(back, EventWire { meta: Meta { id: 3, at: 7 }, kind: Kind::Followed { user: 4 } });
    }

    #[test]
    fn unknown_fields_are_carried_along() {
        let loose: Loose = serde_json::from_str(r#"{"id":3,"color":"red","n":[1]}"#).unwrap();
        assert_eq!(loose.id, 3);
        assert_eq!(loose.extra.get("color"), Some(&json!("red")));
        assert_eq!(loose.extra.get("n"), Some(&json!([1])));
        let again: Value = serde_json::to_value(&loose).unwrap();
        assert_eq!(again, json!({"id": 3, "color": "red", "n": [1]}), "лишние поля должны уйти обратно на тот же уровень");
    }

    #[test]
    fn known_fields_are_not_duplicated_in_extra() {
        let loose: Loose = serde_json::from_str(r#"{"id":3}"#).unwrap();
        assert!(loose.extra.is_empty(), "id известен и в extra попадать не должен");
    }
}
