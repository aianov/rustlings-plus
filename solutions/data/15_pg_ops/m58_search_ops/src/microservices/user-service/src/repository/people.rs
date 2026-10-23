use contracts::UserId;
use sqlx::types::Json;

use super::BoxFuture;
use super::pg::{PgUsers, UserRow, id, user_of};
use crate::common::{Settings, User};

const PEOPLE: &str = "SELECT id, username, display_name, bio FROM users WHERE NOT hidden AND NOT deleted AND (username % $1 OR similarity(username, $1) >= $3) ORDER BY similarity(username, $1) DESC, id LIMIT $2";
const SETTINGS: &str = "SELECT settings FROM users WHERE id = $1 AND NOT deleted";
const PATCH: &str =
    "UPDATE users SET settings = settings || $2 WHERE id = $1 AND NOT deleted RETURNING settings";

pub const TYPO_SIMILARITY: f32 = 0.2;

pub trait ProfileExtras: Send + Sync {
    fn people(&self, query: String, limit: usize) -> BoxFuture<'_, Vec<User>>;
    fn settings(&self, user: UserId) -> BoxFuture<'_, Option<Settings>>;
    fn patch_settings(&self, user: UserId, patch: Settings) -> BoxFuture<'_, Option<Settings>>;
}

impl ProfileExtras for PgUsers {
    fn people(&self, query: String, limit: usize) -> BoxFuture<'_, Vec<User>> {
        Box::pin(async move {
            let rows: Vec<UserRow> = sqlx::query_as(PEOPLE)
                .bind(query.to_lowercase())
                .bind(i64::try_from(limit).unwrap_or(i64::MAX))
                .bind(TYPO_SIMILARITY)
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter().filter_map(user_of).collect()
        })
    }

    fn settings(&self, user: UserId) -> BoxFuture<'_, Option<Settings>> {
        Box::pin(async move {
            let found: Option<Json<Settings>> = sqlx::query_scalar(SETTINGS)
                .bind(id(user))
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
            found.map(|Json(settings)| settings)
        })
    }

    fn patch_settings(&self, user: UserId, patch: Settings) -> BoxFuture<'_, Option<Settings>> {
        Box::pin(async move {
            let saved: Option<Json<Settings>> = sqlx::query_scalar(PATCH)
                .bind(id(user))
                .bind(Json(patch))
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
            saved.map(|Json(settings)| settings)
        })
    }
}
