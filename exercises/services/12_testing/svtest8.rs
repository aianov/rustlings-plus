// Сервис постов переименовал в событии поле `user_id` в `author`. Его тесты зелёные: они читают событие собственным типом.
// А feed-service читает `user_id` и молча перестал показывать посты. Исправили так: потребитель записывает, что ему нужно
// от события (`Expect`: путь и тип), и производитель в своих тестах прогоняет настоящие события через эти ожидания. Но
// `check_contract` проверяет лишь, что первая часть пути есть среди ключей, а типы и вложенность не смотрит. Цель: она
// возвращает список всех нарушений (пустой, если всё в порядке): нет поля, поле `null`, тип не тот, не хватает вложенного
// поля (`author.id`), элемент массива не того типа (`tags[]`). Лишние поля контракт не запрещает.

use serde_json::Value;

fn main() {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ty {
    Str,
    Num,
    Bool,
}

pub struct Expect {
    pub path: &'static str,
    pub ty: Ty,
}

pub fn check_contract(expectations: &[Expect], sample: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for expect in expectations {
        let first = expect.path.split(['.', '[']).next().unwrap_or("");
        if sample.get(first).is_none() {
            found.push(format!("`{}`: нет поля", expect.path));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FEED_NEEDS: &[Expect] = &[
        Expect { path: "id", ty: Ty::Str },
        Expect { path: "user_id", ty: Ty::Num },
        Expect { path: "author.name", ty: Ty::Str },
        Expect { path: "tags[]", ty: Ty::Str },
        Expect { path: "pinned", ty: Ty::Bool },
    ];

    fn good() -> Value {
        json!({"id": "p1", "user_id": 7, "author": {"name": "Ann", "avatar": "x.png"}, "tags": ["a", "b"], "pinned": false, "extra": [1, 2, 3]})
    }

    #[test]
    fn a_matching_event_with_extra_fields_is_fine() {
        assert_eq!(check_contract(FEED_NEEDS, &good()), Vec::<String>::new(), "лишние поля контракт не запрещает: производитель вправе добавлять");
    }

    #[test]
    fn a_renamed_field_is_caught_and_named() {
        let mut event = good();
        let moved = event["user_id"].take();
        event["author_id"] = moved;
        event.as_object_mut().unwrap().remove("user_id");
        let found = check_contract(FEED_NEEDS, &event);
        assert_eq!(found.len(), 1, "нарушение одно: {found:?}");
        assert!(found[0].contains("user_id") && found[0].contains("нет поля"), "сообщение называет поле: {:?}", found[0]);
    }

    #[test]
    fn a_wrong_type_is_a_violation_with_both_types() {
        let mut event = good();
        event["user_id"] = json!("7");
        let found = check_contract(FEED_NEEDS, &event);
        assert_eq!(found.len(), 1, "число превратилось в строку: {found:?}");
        assert!(found[0].contains("число") && found[0].contains("строка"), "в сообщении ожидаемый и пришедший тип: {:?}", found[0]);
    }

    #[test]
    fn nested_fields_are_checked_by_their_full_path() {
        let mut event = good();
        event["author"] = json!({"avatar": "x.png"});
        let found = check_contract(FEED_NEEDS, &event);
        assert_eq!(found.len(), 1, "у автора пропало имя: {found:?}");
        assert!(found[0].contains("author.name"), "путь вложенного поля полный: {:?}", found[0]);
        event["author"] = json!("Ann");
        let found = check_contract(FEED_NEEDS, &event);
        assert!(found.iter().any(|m| m.contains("author.name")), "автор оказался строкой вместо объекта: нарушение, а не паника: {found:?}");
    }

    #[test]
    fn null_counts_as_missing() {
        let mut event = good();
        event["pinned"] = Value::Null;
        let found = check_contract(FEED_NEEDS, &event);
        assert_eq!(found.len(), 1, "null вместо обязательного значения: {found:?}");
        assert!(found[0].contains("pinned"), "названо поле: {:?}", found[0]);
    }

    #[test]
    fn every_array_element_is_checked() {
        let mut event = good();
        event["tags"] = json!(["ok", 5, "fine", true]);
        let found = check_contract(FEED_NEEDS, &event);
        assert_eq!(found.len(), 2, "плохих элементов два: {found:?}");
        assert!(found.iter().any(|m| m.contains("tags[1]")) && found.iter().any(|m| m.contains("tags[3]")), "с номерами элементов: {found:?}");
        event["tags"] = json!([]);
        assert!(check_contract(FEED_NEEDS, &event).is_empty(), "пустой массив это не нарушение");
        event["tags"] = json!("a,b");
        assert!(!check_contract(FEED_NEEDS, &event).is_empty(), "вместо массива строка: нарушение");
    }

    #[test]
    fn all_violations_are_reported_together() {
        let found = check_contract(FEED_NEEDS, &json!({"id": 1, "tags": [1], "pinned": "yes"}));
        assert!(found.len() >= 5, "тут пять нарушений, а не первое найденное: {found:?}");
    }

    #[test]
    fn a_non_object_event_does_not_crash_the_check() {
        let found = check_contract(FEED_NEEDS, &json!([1, 2, 3]));
        assert!(found.len() >= 5, "массив вместо события: каждое ожидание нарушено, паники нет: {found:?}");
    }
}
