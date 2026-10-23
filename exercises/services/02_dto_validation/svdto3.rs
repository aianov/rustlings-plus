// Пользователь обновляет профиль, и сервер разбирает тело прямо в `User`, а потом сохраняет. Значит,
// клиент может прислать `{"role":"admin"}` или чужой `id`, и они запишутся. Тест ниже делает именно это.
// Цель: клиент меняет только `displayName` и `bio`; попытка прислать что-то ещё отклоняется ошибкой,
// которая называет поле, и пользователь остаётся нетронутым. Сигнатуру `apply_profile_update` не меняй.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u64,
    pub role: String,
    pub display_name: String,
    pub bio: String,
}

#[derive(Deserialize)]
struct UserPatch {
    id: Option<u64>,
    role: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    bio: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct Rejected(pub String);

pub fn apply_profile_update(user: &mut User, body: &str) -> Result<(), Rejected> {
    let patch: UserPatch = serde_json::from_str(body).map_err(|e| Rejected(e.to_string()))?;
    if let Some(id) = patch.id {
        user.id = id;
    }
    if let Some(role) = patch.role {
        user.role = role;
    }
    if let Some(name) = patch.display_name {
        user.display_name = name;
    }
    if let Some(bio) = patch.bio {
        user.bio = bio;
    }
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn member() -> User {
        User { id: 7, role: "member".to_string(), display_name: "Bob".to_string(), bio: "hi".to_string() }
    }

    #[test]
    fn legitimate_update_works() {
        let mut user = member();
        apply_profile_update(&mut user, r#"{"displayName":"Robert","bio":"new"}"#).unwrap();
        assert_eq!(user.display_name, "Robert", "displayName меняется");
        assert_eq!(user.bio, "new", "bio меняется");
        assert_eq!((user.id, user.role.as_str()), (7, "member"), "id и роль остаются прежними");
    }

    #[test]
    fn partial_update_keeps_other_fields() {
        let mut user = member();
        apply_profile_update(&mut user, r#"{"bio":"only bio"}"#).unwrap();
        assert_eq!(user.display_name, "Bob", "не присланное поле не трогаем");
    }

    #[test]
    fn role_cannot_be_set_by_the_client() {
        let mut user = member();
        let err = apply_profile_update(&mut user, r#"{"bio":"x","role":"admin"}"#)
            .expect_err("попытка сменить роль должна отклоняться ошибкой, а не игнорироваться");
        assert!(err.0.contains("role"), "ошибка должна называть поле role, а сказала: {}", err.0);
        assert_eq!(user.role, "member", "роль не изменилась");
        assert_eq!(user.bio, "hi", "при отказе профиль не меняется вообще, даже допустимыми полями");
    }

    #[test]
    fn id_cannot_be_set_by_the_client() {
        let mut user = member();
        assert!(apply_profile_update(&mut user, r#"{"id":1}"#).is_err(), "чужой id менять нельзя");
        assert_eq!(user.id, 7, "id не изменился");
    }
}
