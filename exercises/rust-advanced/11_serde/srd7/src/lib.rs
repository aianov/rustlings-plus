// В ленте три вида идентификаторов, и путать их легко. `UserId` это просто число на проводе (newtype serde
// пишет как внутреннее значение). `Handle` структура с одним полем, которая на проводе должна выглядеть как
// голая строка, без вложенного объекта. А `PostId` на проводе строка вида `p_42`: так её не спутают с числами
// других сущностей, а JS-клиент не потеряет точность. В Rust это по-прежнему `PostId(u64)`.
// TODO: Заставь `Handle` и `PostId` выглядеть на проводе как описано. Ошибка разбора должна объяснять формат.
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct UserId(pub u64);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Handle {
    raw: String,
}

impl Handle {
    pub fn new(raw: &str) -> Handle {
        Handle { raw: raw.to_string() }
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct PostId(pub u64);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Post {
    pub id: PostId,
    pub author: UserId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_id_is_a_bare_number() {
        assert_eq!(serde_json::to_string(&UserId(3)).unwrap(), "3");
        assert_eq!(serde_json::from_str::<UserId>("3").unwrap(), UserId(3));
    }

    #[test]
    fn handle_is_a_bare_string() {
        assert_eq!(serde_json::to_string(&Handle::new("abc")).unwrap(), r#""abc""#, "лишний объект вокруг строки");
        assert_eq!(serde_json::from_str::<Handle>(r#""zz""#).unwrap().as_str(), "zz");
    }

    #[test]
    fn post_id_is_a_prefixed_string() {
        assert_eq!(serde_json::to_string(&PostId(7)).unwrap(), r#""p_7""#);
        assert_eq!(serde_json::from_str::<PostId>(r#""p_42""#).unwrap(), PostId(42));
        let post = Post { id: PostId(7), author: UserId(3) };
        assert_eq!(serde_json::to_string(&post).unwrap(), r#"{"id":"p_7","author":3}"#);
        assert_eq!(serde_json::from_str::<Post>(r#"{"id":"p_7","author":3}"#).unwrap(), post);
    }

    #[test]
    fn bad_ids_are_explained() {
        for raw in [r#""42""#, r#""u_42""#, r#""p_""#, r#""p_x""#, r#""p_-1""#] {
            let err = serde_json::from_str::<PostId>(raw).expect_err("такой id принимать нельзя");
            assert!(err.to_string().contains("p_<number>"), "ошибка для {raw} должна объяснять формат: {err}");
        }
        assert!(serde_json::from_str::<PostId>("42").is_err(), "число вместо строки тоже отказ");
    }
}
