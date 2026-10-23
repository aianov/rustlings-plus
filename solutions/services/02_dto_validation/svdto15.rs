use serde::{Deserialize, Deserializer};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Patch<T> {
    Missing,
    Null,
    Value(T),
}

#[allow(clippy::derivable_impls)]
impl<T> Default for Patch<T> {
    fn default() -> Self {
        Patch::Missing
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Option::<T>::deserialize(deserializer)? {
            None => Patch::Null,
            Some(value) => Patch::Value(value),
        })
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePatch {
    #[serde(default)]
    pub display_name: Patch<String>,
    #[serde(default)]
    pub bio: Patch<String>,
    #[serde(default)]
    pub birth_year: Patch<u16>,
}

pub fn parse_patch(json: &str) -> Result<ProfilePatch, serde_json::Error> {
    serde_json::from_str(json)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_fields_are_missing() {
        let patch = parse_patch("{}").unwrap();
        assert_eq!(
            patch,
            ProfilePatch { display_name: Patch::Missing, bio: Patch::Missing, birth_year: Patch::Missing },
            "пустой PATCH не трогает ничего"
        );
    }

    #[test]
    fn explicit_null_is_not_the_same_as_absent() {
        let patch = parse_patch(r#"{"bio":null}"#).unwrap();
        assert_eq!(patch.bio, Patch::Null, "null означает «очисти поле»");
        assert_eq!(patch.display_name, Patch::Missing, "соседнее поле не прислано");
    }

    #[test]
    fn values_are_carried() {
        let patch = parse_patch(r#"{"displayName":"Alice","birthYear":1999}"#).unwrap();
        assert_eq!(patch.display_name, Patch::Value("Alice".to_string()), "строка");
        assert_eq!(patch.birth_year, Patch::Value(1999), "число");
    }

    #[test]
    fn empty_string_is_a_value_not_null() {
        let patch = parse_patch(r#"{"bio":""}"#).unwrap();
        assert_eq!(patch.bio, Patch::Value(String::new()), "пустая строка это значение, а не null");
    }

    #[test]
    fn wrong_types_still_fail() {
        assert!(parse_patch(r#"{"birthYear":"1999"}"#).is_err(), "строка вместо числа это ошибка разбора");
    }

    #[test]
    fn patch_works_for_any_type() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Wrapper {
            #[serde(default)]
            tags: Patch<Vec<String>>,
        }
        let w: Wrapper = serde_json::from_str(r#"{"tags":["a","b"]}"#).unwrap();
        assert_eq!(w.tags, Patch::Value(vec!["a".to_string(), "b".to_string()]), "вектор внутри Patch");
        let w: Wrapper = serde_json::from_str(r#"{"tags":null}"#).unwrap();
        assert_eq!(w.tags, Patch::Null, "null для вектора");
    }
}
