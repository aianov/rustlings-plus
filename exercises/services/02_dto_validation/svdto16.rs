// Клиент шлёт `PATCH {"bio": "новое био"}` и теряет имя и год рождения: они стёрлись. А когда шлёт
// `{"bio": null}`, чтобы очистить био, ничего не происходит. Тип поля не различает «не прислано»
// и `null`. Цель: `apply_patch` меняет только присланные поля; `null` очищает `bio` и
// `birthYear`, а для `displayName` это ошибка `null_not_allowed`; значения проверяются:
// displayName 1..=30 символов после trim, bio 1..=160 (пустое био очищают через `null`), birthYear
// 1900..=2020. Ошибки собираются все, и при любой ошибке профиль не меняется вообще.

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

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub display_name: String,
    pub bio: Option<String>,
    pub birth_year: Option<u16>,
}

#[derive(Debug, PartialEq)]
pub struct FieldError {
    pub field: &'static str,
    pub code: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyPatch {
    display_name: Option<String>,
    bio: Option<String>,
    birth_year: Option<u16>,
}

pub fn apply_patch(profile: &mut Profile, body: &str) -> Result<(), Vec<FieldError>> {
    let patch: LegacyPatch = serde_json::from_str(body)
        .map_err(|_| vec![FieldError { field: "body", code: "bad_json" }])?;
    profile.display_name = patch.display_name.unwrap_or_default();
    profile.bio = patch.bio;
    profile.birth_year = patch.birth_year;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        Profile { display_name: "Alice".to_string(), bio: Some("люблю Rust".to_string()), birth_year: Some(1999) }
    }

    #[test]
    fn patch_touches_only_sent_fields() {
        let mut p = profile();
        apply_patch(&mut p, r#"{"bio":"новое био"}"#).unwrap();
        assert_eq!(p.bio.as_deref(), Some("новое био"), "bio обновилось");
        assert_eq!(p.display_name, "Alice", "имя не прислано и не должно измениться");
        assert_eq!(p.birth_year, Some(1999), "год не прислан и не должен измениться");
    }

    #[test]
    fn null_clears_nullable_fields() {
        let mut p = profile();
        apply_patch(&mut p, r#"{"bio":null,"birthYear":null}"#).unwrap();
        assert_eq!((p.bio, p.birth_year), (None, None), "null очищает bio и birthYear");
        assert_eq!(p.display_name, "Alice", "имя на месте");
    }

    #[test]
    fn null_display_name_is_an_error() {
        let mut p = profile();
        let errors = apply_patch(&mut p, r#"{"displayName":null}"#).unwrap_err();
        assert_eq!(errors, vec![FieldError { field: "displayName", code: "null_not_allowed" }], "имя очищать нельзя");
        assert_eq!(p, profile(), "профиль не изменился");
    }

    #[test]
    fn empty_patch_changes_nothing() {
        let mut p = profile();
        apply_patch(&mut p, "{}").unwrap();
        assert_eq!(p, profile(), "пустой PATCH ничего не меняет");
    }

    #[test]
    fn invalid_values_are_all_reported_and_nothing_is_applied() {
        let mut p = profile();
        let errors = apply_patch(&mut p, r#"{"displayName":"  ","bio":"","birthYear":1800}"#).unwrap_err();
        let codes: Vec<_> = errors.iter().map(|e| (e.field, e.code)).collect();
        assert_eq!(
            codes,
            vec![("displayName", "too_short"), ("bio", "too_short"), ("birthYear", "out_of_range")],
            "все три ошибки сразу, в порядке displayName, bio, birthYear"
        );
        assert_eq!(p, profile(), "при ошибках профиль не меняется ни частично, ни совсем");
    }

    #[test]
    fn valid_and_invalid_mix_is_atomic() {
        let mut p = profile();
        assert!(apply_patch(&mut p, r#"{"displayName":"Bob","bio":"x"}"#).is_ok(), "оба допустимы");
        let mut p = profile();
        assert!(apply_patch(&mut p, r#"{"displayName":"Bob","birthYear":3000}"#).is_err(), "год плох");
        assert_eq!(p.display_name, "Alice", "хорошее поле из плохого запроса тоже не применилось");
    }

    #[test]
    fn values_are_trimmed_and_bad_json_is_reported() {
        let mut p = profile();
        apply_patch(&mut p, r#"{"displayName":"  Bob  "}"#).unwrap();
        assert_eq!(p.display_name, "Bob", "имя обрезается");
        let errors = apply_patch(&mut p, "{oops").unwrap_err();
        assert_eq!(errors, vec![FieldError { field: "body", code: "bad_json" }], "битое тело");
    }
}
