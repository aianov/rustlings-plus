use std::io;
use std::sync::Arc;

use contracts::{Cause, Fact, Followed, Muted, ProfileChanged, Registered, UserId, Username};
use db_engine::{DbError, Migration, OUTBOX_SQL, record_fact};
use journal::JournalError;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::Notify;

use super::{BoxFuture, RepoError, UserRepository};
use crate::common::{Bio, DisplayName, ProfileChange, Relations, User};

pub const MIGRATIONS: [Migration; 2] = [
    ("0000_outbox", OUTBOX_SQL),
    (
        "0001_init.sql",
        include_str!("../../migrations/0001_init.sql"),
    ),
];

pub const USERNAME_TAKEN: &str = "users_username_key";

const INSERT_USER: &str = "INSERT INTO users (username) VALUES ($1) RETURNING id";
const SELECT_USER: &str = "SELECT id, username, display_name, bio FROM users WHERE id = $1 AND NOT hidden AND NOT deleted";
const SELECT_BY_NAME: &str = "SELECT id, username, display_name, bio FROM users WHERE username = $1 AND NOT hidden AND NOT deleted";
const SELECT_PAGE: &str = "SELECT id, username, display_name, bio FROM users WHERE id > $1 AND NOT hidden AND NOT deleted ORDER BY id LIMIT $2";
const INSERT_FOLLOW: &str =
    "INSERT INTO follows (follower, followee) VALUES ($1, $2) ON CONFLICT DO NOTHING";
const INSERT_MUTE: &str =
    "INSERT INTO mutes (user_id, muted) VALUES ($1, $2) ON CONFLICT DO NOTHING";
const SELECT_FOLLOWING: &str = "SELECT followee FROM follows WHERE follower = $1 ORDER BY followee";
const SELECT_FOLLOWERS: &str = "SELECT follower FROM follows WHERE followee = $1 ORDER BY follower";
const SELECT_MUTED: &str = "SELECT muted FROM mutes WHERE user_id = $1 ORDER BY muted";
const SELECT_AUDIENCE: &str = "SELECT f.follower FROM follows f WHERE f.followee = $1 AND NOT EXISTS (SELECT 1 FROM mutes m WHERE m.user_id = f.follower AND m.muted = $1) ORDER BY f.follower";
const UPDATE_PROFILE: &str = "UPDATE users SET display_name = $2, bio = $3 WHERE id = $1";
pub(super) const HIDE_USER: &str =
    "UPDATE users SET hidden = true WHERE id = $1 AND NOT hidden AND NOT deleted";
pub(super) const SHOW_USER: &str =
    "UPDATE users SET hidden = false WHERE id = $1 AND hidden AND NOT deleted";
pub(super) const ERASE_USER: &str = "UPDATE users SET deleted = true, hidden = false, display_name = NULL, bio = NULL WHERE id = $1 AND NOT deleted";
pub(super) const ERASE_FOLLOWS: &str = "DELETE FROM follows WHERE follower = $1 OR followee = $1";
pub(super) const ERASE_MUTES: &str = "DELETE FROM mutes WHERE user_id = $1 OR muted = $1";

#[derive(sqlx::FromRow)]
pub(super) struct UserRow {
    id: i64,
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
}

pub(super) fn storage(error: impl std::fmt::Display) -> JournalError {
    JournalError::Io(io::Error::other(error.to_string()))
}

pub(super) fn id(user: UserId) -> i64 {
    i64::try_from(user.0).unwrap_or(i64::MAX)
}

pub(super) fn user_of(row: UserRow) -> Option<User> {
    Some(User {
        id: UserId(u64::try_from(row.id).ok()?),
        name: Username::try_from(row.username.as_str()).ok()?,
        display_name: row
            .display_name
            .and_then(|name| DisplayName::try_from(name.as_str()).ok()),
        bio: row.bio.and_then(|bio| Bio::try_from(bio.as_str()).ok()),
    })
}

fn ids(rows: Vec<i64>) -> Vec<UserId> {
    rows.into_iter()
        .filter_map(|raw| u64::try_from(raw).ok())
        .map(UserId)
        .collect()
}

pub struct PgUsers {
    pub(super) pool: PgPool,
    pub(super) signal: Arc<Notify>,
}

impl PgUsers {
    pub fn new(pool: PgPool) -> PgUsers {
        PgUsers {
            pool,
            signal: Arc::default(),
        }
    }

    pub(super) async fn commit(
        &self,
        mut tx: Transaction<'_, Postgres>,
        fact: Option<Fact>,
        cause: &Cause,
    ) -> Result<(), DbError> {
        if let Some(fact) = fact {
            record_fact(&mut tx, &fact, cause).await?;
        }
        tx.commit().await?;
        self.signal.notify_one();
        Ok(())
    }

    async fn by_id(&self, user: UserId) -> Option<User> {
        let row: Option<UserRow> = sqlx::query_as(SELECT_USER)
            .bind(id(user))
            .fetch_optional(&self.pool)
            .await
            .ok()?;
        row.and_then(user_of)
    }

    async fn by_name(&self, name: &str) -> Option<User> {
        let row: Option<UserRow> = sqlx::query_as(SELECT_BY_NAME)
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .ok()?;
        row.and_then(user_of)
    }

    async fn pair(
        &self,
        query: &str,
        who: UserId,
        whom: UserId,
        fact: Fact,
        cause: &Cause,
    ) -> Result<bool, DbError> {
        let mut tx = self.pool.begin().await?;
        let added = sqlx::query(query)
            .bind(id(who))
            .bind(id(whom))
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        self.commit(tx, added.then_some(fact), cause).await?;
        Ok(added)
    }

    async fn list(&self, query: &str, user: UserId) -> Vec<UserId> {
        let rows: Vec<i64> = sqlx::query_scalar(query)
            .bind(id(user))
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
        ids(rows)
    }
}

impl UserRepository for PgUsers {
    fn create_if_absent(
        &self,
        name: Username,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<User, RepoError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let attempt = async {
                let mut tx = self.pool.begin().await?;
                let raw: i64 = sqlx::query_scalar(INSERT_USER)
                    .bind(name.as_ref())
                    .fetch_one(&mut *tx)
                    .await?;
                let user = User::new(UserId(u64::try_from(raw).unwrap_or(0)), name.clone());
                let fact = Fact::Registered(Registered {
                    user_id: user.id,
                    username: name.as_ref().to_string(),
                });
                self.commit(tx, Some(fact), &cause).await?;
                Ok::<User, DbError>(user)
            };
            match attempt.await {
                Ok(user) => Ok(user),
                Err(error) if error.is_unique(USERNAME_TAKEN) => Err(RepoError::Conflict),
                Err(error) => Err(RepoError::Storage(storage(error))),
            }
        })
    }

    fn get(&self, user: UserId) -> BoxFuture<'_, Option<User>> {
        Box::pin(async move { self.by_id(user).await })
    }

    fn find(&self, name: Username) -> BoxFuture<'_, Option<User>> {
        Box::pin(async move { self.by_name(name.as_ref()).await })
    }

    fn list_after(&self, after: u64, limit: usize) -> BoxFuture<'_, Vec<User>> {
        Box::pin(async move {
            let rows: Vec<UserRow> = sqlx::query_as(SELECT_PAGE)
                .bind(i64::try_from(after).unwrap_or(i64::MAX))
                .bind(i64::try_from(limit).unwrap_or(i64::MAX))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter().filter_map(user_of).collect()
        })
    }

    fn follow(
        &self,
        who: UserId,
        whom: UserId,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let fact = Fact::Followed(Followed {
                follower_id: who,
                followee_id: whom,
            });
            self.pair(INSERT_FOLLOW, who, whom, fact, &cause)
                .await
                .map_err(storage)
        })
    }

    fn mute(
        &self,
        who: UserId,
        whom: UserId,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let fact = Fact::Muted(Muted {
                user_id: who,
                muted_id: whom,
            });
            self.pair(INSERT_MUTE, who, whom, fact, &cause)
                .await
                .map_err(storage)
        })
    }

    fn relations(&self, user: UserId) -> BoxFuture<'_, Relations> {
        Box::pin(async move {
            Relations {
                following: self.list(SELECT_FOLLOWING, user).await,
                followers: self.list(SELECT_FOLLOWERS, user).await,
                muted: self.list(SELECT_MUTED, user).await,
                audience: self.list(SELECT_AUDIENCE, user).await,
            }
        })
    }

    fn update(
        &self,
        user: UserId,
        change: ProfileChange,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<User>, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let Some(current) = self.by_id(user).await else {
                return Ok(None);
            };
            let next = change.apply(&current);
            if next == current {
                return Ok(Some(next));
            }
            let display_name = next
                .display_name
                .as_ref()
                .map(|name| name.as_ref().to_string());
            let bio = next.bio.as_ref().map(|bio| bio.as_ref().to_string());
            let saving = async {
                let mut tx = self.pool.begin().await?;
                sqlx::query(UPDATE_PROFILE)
                    .bind(id(user))
                    .bind(&display_name)
                    .bind(&bio)
                    .execute(&mut *tx)
                    .await?;
                let fact = Fact::ProfileChanged(ProfileChanged {
                    user_id: user,
                    display_name: display_name.clone(),
                });
                self.commit(tx, Some(fact), &cause).await
            };
            saving.await.map_err(storage)?;
            Ok(Some(next))
        })
    }
}
