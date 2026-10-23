use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UserId(pub u64);

#[derive(Clone, Debug)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub banned: bool,
}

#[derive(Debug, PartialEq)]
pub enum UserError {
    NotFound(UserId),
    Banned(UserId),
}

pub struct UserService {
    users: HashMap<UserId, User>,
}

impl UserService {
    pub fn new(users: Vec<User>) -> Self {
        Self { users: users.into_iter().map(|u| (u.id, u)).collect() }
    }

    pub fn get(&self, id: UserId) -> Result<User, UserError> {
        match self.users.get(&id) {
            None => Err(UserError::NotFound(id)),
            Some(u) if u.banned => Err(UserError::Banned(id)),
            Some(u) => Ok(u.clone()),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

#[derive(Serialize)]
pub struct UserDto {
    pub id: u64,
    pub name: String,
}

impl From<User> for UserDto {
    fn from(user: User) -> Self {
        UserDto { id: user.id.0, name: user.name }
    }
}

pub fn parse_id(raw: &str) -> Result<UserId, ApiError> {
    raw.parse().map(UserId).map_err(|_| ApiError { status: 400, code: "bad_id" })
}

impl From<UserError> for ApiError {
    fn from(err: UserError) -> Self {
        match err {
            UserError::NotFound(_) => ApiError { status: 404, code: "user_not_found" },
            UserError::Banned(_) => ApiError { status: 403, code: "user_banned" },
        }
    }
}

pub fn get_user(service: &UserService, raw_id: &str) -> Result<UserDto, ApiError> {
    let id = parse_id(raw_id)?;
    Ok(service.get(id)?.into())
}

pub fn render<T: Serialize>(result: Result<T, ApiError>) -> Response {
    match result {
        Ok(value) => Response { status: 200, body: serde_json::to_string(&value).unwrap() },
        Err(err) => Response {
            status: err.status,
            body: serde_json::json!({ "error": err.code }).to_string(),
        },
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> UserService {
        UserService::new(vec![
            User { id: UserId(1), name: "alice".to_string(), banned: false },
            User { id: UserId(2), name: "mallory".to_string(), banned: true },
        ])
    }

    #[test]
    fn happy_path() {
        let resp = render(get_user(&service(), "1"));
        assert_eq!(resp.status, 200, "найденный пользователь это 200");
        assert_eq!(resp.body, r#"{"id":1,"name":"alice"}"#, "тело это UserDto в JSON");
    }

    #[test]
    fn garbage_id_is_400_before_service_is_called() {
        for raw in ["abc", "", "-1", "1.5", "99999999999999999999999"] {
            let err = parse_id(raw).unwrap_err();
            assert_eq!(err, ApiError { status: 400, code: "bad_id" }, "id {raw:?} разбирается только в контроллере");
        }
        assert_eq!(parse_id("42"), Ok(UserId(42)), "нормальный id");
    }

    #[test]
    fn domain_errors_map_to_statuses() {
        let resp = render(get_user(&service(), "7"));
        assert_eq!(
            resp,
            Response { status: 404, body: r#"{"error":"user_not_found"}"#.to_string() },
            "неизвестный пользователь"
        );
        let resp = render(get_user(&service(), "2"));
        assert_eq!(
            resp,
            Response { status: 403, body: r#"{"error":"user_banned"}"#.to_string() },
            "забаненный пользователь"
        );
        let resp = render(get_user(&service(), "x"));
        assert_eq!(resp.status, 400, "битый id это 400");
    }

    #[test]
    fn render_is_generic() {
        let ok: Result<Vec<u64>, ApiError> = Ok(vec![1, 2, 3]);
        assert_eq!(render(ok).body, "[1,2,3]", "render принимает любой Serialize, не только UserDto");
        let err: Result<(), ApiError> = Err(ApiError { status: 418, code: "teapot" });
        assert_eq!(
            render(err),
            Response { status: 418, body: r#"{"error":"teapot"}"#.to_string() },
            "ошибка несёт свой статус и код"
        );
    }
}
