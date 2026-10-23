use std::sync::Arc;

use contracts::{OutboxRow, UserId};
use service_runtime::StorageError;
use tokio::sync::Notify;

use super::pg::{PgUsers, id, storage};
use super::{AvatarRepository, BoxFuture, OutboxRepository};
use crate::common::Avatar;

const SELECT_AVATAR: &str = "SELECT file_key, content_type FROM avatars WHERE owner = $1";
const UPSERT_AVATAR: &str = "INSERT INTO avatars (owner, file_key, content_type) VALUES ($1, $2, $3) ON CONFLICT (owner) DO UPDATE SET file_key = EXCLUDED.file_key, content_type = EXCLUDED.content_type";
const DELETE_AVATAR: &str = "DELETE FROM avatars WHERE owner = $1 RETURNING file_key, content_type";
const REFERENCED: &str = "SELECT EXISTS (SELECT 1 FROM avatars WHERE file_key = $1)";
const PENDING: &str = "INSERT INTO pending_files (file_key, at) VALUES ($1, $2) ON CONFLICT (file_key) DO UPDATE SET at = EXCLUDED.at";
const SETTLE: &str = "DELETE FROM pending_files WHERE file_key = $1";
const STALE: &str = "SELECT file_key FROM pending_files WHERE at < $1 ORDER BY at";

#[derive(sqlx::FromRow)]
struct AvatarRow {
    file_key: String,
    content_type: String,
}

fn avatar(row: AvatarRow) -> Avatar {
    Avatar {
        key: row.file_key,
        content_type: row.content_type,
    }
}

impl AvatarRepository for PgUsers {
    fn link(&self, owner: UserId, next: Avatar) -> BoxFuture<'_, Option<Avatar>> {
        Box::pin(async move {
            let previous: Option<AvatarRow> = sqlx::query_as(SELECT_AVATAR)
                .bind(id(owner))
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
            let _ = sqlx::query(UPSERT_AVATAR)
                .bind(id(owner))
                .bind(&next.key)
                .bind(&next.content_type)
                .execute(&self.pool)
                .await;
            previous.map(avatar)
        })
    }

    fn get(&self, owner: UserId) -> BoxFuture<'_, Option<Avatar>> {
        Box::pin(async move {
            let row: Option<AvatarRow> = sqlx::query_as(SELECT_AVATAR)
                .bind(id(owner))
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
            row.map(avatar)
        })
    }

    fn remove(&self, owner: UserId) -> BoxFuture<'_, Option<Avatar>> {
        Box::pin(async move {
            let row: Option<AvatarRow> = sqlx::query_as(DELETE_AVATAR)
                .bind(id(owner))
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
            row.map(avatar)
        })
    }

    fn referenced(&self, key: String) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            sqlx::query_scalar(REFERENCED)
                .bind(key)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(true)
        })
    }

    fn pending(&self, key: String, at: u64) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _ = sqlx::query(PENDING)
                .bind(key)
                .bind(i64::try_from(at).unwrap_or(i64::MAX))
                .execute(&self.pool)
                .await;
        })
    }

    fn settle(&self, key: String) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _ = sqlx::query(SETTLE).bind(key).execute(&self.pool).await;
        })
    }

    fn stale(&self, before: u64) -> BoxFuture<'_, Vec<String>> {
        Box::pin(async move {
            sqlx::query_scalar(STALE)
                .bind(i64::try_from(before).unwrap_or(i64::MAX))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default()
        })
    }
}

pub const CLAIM_HOLD: std::time::Duration = std::time::Duration::from_secs(30);

impl OutboxRepository for PgUsers {
    fn unpublished(&self, limit: usize) -> BoxFuture<'_, Vec<OutboxRow>> {
        Box::pin(async move {
            db_engine::claim(&self.pool, &self.owner, limit, CLAIM_HOLD)
                .await
                .unwrap_or_default()
        })
    }

    fn mark_published(&self, through: u64) -> BoxFuture<'_, Result<(), StorageError>> {
        Box::pin(async move {
            db_engine::release(&self.pool, &self.owner, through)
                .await
                .map_err(storage)
        })
    }

    fn waiting(&self) -> BoxFuture<'_, usize> {
        Box::pin(async move { db_engine::waiting(&self.pool).await.unwrap_or(0) })
    }

    fn signal(&self) -> Arc<Notify> {
        Arc::clone(&self.signal)
    }
}
