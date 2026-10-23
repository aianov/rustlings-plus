use db_engine::DbError;

use super::pg::{PgAccounts, big, small};
use super::{BoxFuture, Opening, RefreshError, SessionRepository, SessionRow};

const NEW_FAMILY: &str = "INSERT INTO session_families (user_id) VALUES ($1) RETURNING family";
const INSERT_SESSION: &str = "INSERT INTO sessions (refresh_hash, family, user_id, expires, access_jti, access_exp) VALUES ($1, $2, $3, $4, $5, $6)";
const LOCK_SESSION: &str = "SELECT s.family, s.user_id, s.used, s.expires, s.access_jti, s.access_exp, f.revoked FROM sessions s JOIN session_families f ON f.family = s.family WHERE s.refresh_hash = $1 FOR UPDATE OF s";
const USE_SESSION: &str = "UPDATE sessions SET used = true WHERE refresh_hash = $1";
const SELECT_SESSION: &str = "SELECT family, user_id, used, expires, access_jti, access_exp FROM sessions WHERE refresh_hash = $1";
const SELECT_BY_JTI: &str = "SELECT family, user_id, used, expires, access_jti, access_exp FROM sessions WHERE access_jti = $1";
const REVOKE_FAMILY: &str = "UPDATE session_families SET revoked = true WHERE family = $1";
const DENY_FAMILY: &str = "INSERT INTO denied_tokens (jti, until) SELECT access_jti, access_exp + $2 FROM sessions WHERE family = $1 ON CONFLICT (jti) DO NOTHING";
const USER_FAMILIES: &str = "SELECT family FROM session_families WHERE user_id = $1 AND NOT revoked AND family IS DISTINCT FROM $2 ORDER BY family";
const DENY: &str = "INSERT INTO denied_tokens (jti, until) VALUES ($1, $2) ON CONFLICT (jti) DO UPDATE SET until = EXCLUDED.until";
const IS_DENIED: &str = "SELECT EXISTS (SELECT 1 FROM denied_tokens WHERE jti = $1)";
const PURGE_DENIED: &str = "DELETE FROM denied_tokens WHERE until <= $1";
const PURGE_SESSIONS: &str = "DELETE FROM sessions WHERE expires <= $1";

#[derive(sqlx::FromRow)]
struct Row {
    family: i64,
    user_id: i64,
    used: bool,
    expires: i64,
    access_jti: String,
    access_exp: i64,
}

#[derive(sqlx::FromRow)]
struct Locked {
    family: i64,
    user_id: i64,
    used: bool,
    expires: i64,
    revoked: bool,
}

fn session(row: Row) -> SessionRow {
    SessionRow {
        family: small(row.family),
        user_id: small(row.user_id),
        used: row.used,
        expires: small(row.expires),
        access_jti: row.access_jti,
        access_exp: small(row.access_exp),
    }
}

impl PgAccounts {
    async fn revoke(&self, family: u64) -> Result<(), DbError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(REVOKE_FAMILY)
            .bind(big(family))
            .execute(&mut *tx)
            .await?;
        sqlx::query(DENY_FAMILY)
            .bind(big(family))
            .bind(big(self.leeway))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn open_in(
        &self,
        family: Option<u64>,
        user_id: u64,
        opening: &Opening,
    ) -> Result<SessionRow, DbError> {
        let mut tx = self.pool.begin().await?;
        let family = match family {
            Some(family) => big(family),
            None => {
                sqlx::query_scalar(NEW_FAMILY)
                    .bind(big(user_id))
                    .fetch_one(&mut *tx)
                    .await?
            }
        };
        sqlx::query(INSERT_SESSION)
            .bind(&opening.refresh_hash)
            .bind(family)
            .bind(big(user_id))
            .bind(big(opening.expires))
            .bind(&opening.access_jti)
            .bind(big(opening.access_exp))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(SessionRow {
            family: small(family),
            user_id,
            used: false,
            expires: opening.expires,
            access_jti: opening.access_jti.clone(),
            access_exp: opening.access_exp,
        })
    }

    async fn turn(
        &self,
        old_hash: &str,
        now: u64,
        opening: &Opening,
    ) -> Result<Result<SessionRow, RefreshError>, DbError> {
        let mut tx = self.pool.begin().await?;
        let locked: Option<Locked> = sqlx::query_as(LOCK_SESSION)
            .bind(old_hash)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(old) = locked else {
            return Ok(Err(RefreshError::Unknown));
        };
        if old.revoked {
            return Ok(Err(RefreshError::Revoked));
        }
        if old.used {
            tx.commit().await?;
            self.revoke(small(old.family)).await?;
            return Ok(Err(RefreshError::Reused));
        }
        if now >= small(old.expires) {
            return Ok(Err(RefreshError::Expired));
        }
        sqlx::query(USE_SESSION)
            .bind(old_hash)
            .execute(&mut *tx)
            .await?;
        sqlx::query(INSERT_SESSION)
            .bind(&opening.refresh_hash)
            .bind(old.family)
            .bind(old.user_id)
            .bind(big(opening.expires))
            .bind(&opening.access_jti)
            .bind(big(opening.access_exp))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Ok(SessionRow {
            family: small(old.family),
            user_id: small(old.user_id),
            used: false,
            expires: opening.expires,
            access_jti: opening.access_jti.clone(),
            access_exp: opening.access_exp,
        }))
    }
}

impl SessionRepository for PgAccounts {
    fn open(&self, user_id: u64, opening: Opening) -> BoxFuture<'_, SessionRow> {
        Box::pin(async move {
            match self.open_in(None, user_id, &opening).await {
                Ok(row) => row,
                Err(..) => SessionRow {
                    family: 0,
                    user_id,
                    used: true,
                    expires: 0,
                    access_jti: opening.access_jti,
                    access_exp: opening.access_exp,
                },
            }
        })
    }

    fn rotate(
        &self,
        old_hash: String,
        now: u64,
        opening: Opening,
    ) -> BoxFuture<'_, Result<SessionRow, RefreshError>> {
        Box::pin(async move {
            self.turn(&old_hash, now, &opening)
                .await
                .unwrap_or(Err(RefreshError::Unknown))
        })
    }

    fn family_of(&self, jti: String) -> BoxFuture<'_, Option<SessionRow>> {
        Box::pin(async move {
            let row: Option<Row> = sqlx::query_as(SELECT_BY_JTI)
                .bind(jti)
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
            row.map(session)
        })
    }

    fn revoke_family(&self, family: u64) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _ = self.revoke(family).await;
        })
    }

    fn revoke_user(&self, user_id: u64, keep: Option<u64>) -> BoxFuture<'_, Vec<u64>> {
        Box::pin(async move {
            let families: Vec<i64> = sqlx::query_scalar(USER_FAMILIES)
                .bind(big(user_id))
                .bind(keep.map(big))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            let mut revoked = Vec::with_capacity(families.len());
            for family in families {
                if self.revoke(small(family)).await.is_ok() {
                    revoked.push(small(family));
                }
            }
            revoked
        })
    }

    fn row(&self, refresh_hash: String) -> BoxFuture<'_, Option<SessionRow>> {
        Box::pin(async move {
            let row: Option<Row> = sqlx::query_as(SELECT_SESSION)
                .bind(refresh_hash)
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
            row.map(session)
        })
    }

    fn deny(&self, jti: String, until: u64) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _ = sqlx::query(DENY)
                .bind(jti)
                .bind(big(until))
                .execute(&self.pool)
                .await;
        })
    }

    fn is_denied(&self, jti: String) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            sqlx::query_scalar(IS_DENIED)
                .bind(jti)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(false)
        })
    }

    fn purge(&self, now: u64) -> BoxFuture<'_, usize> {
        Box::pin(async move {
            let denied = sqlx::query(PURGE_DENIED)
                .bind(big(now))
                .execute(&self.pool)
                .await
                .map(|done| done.rows_affected())
                .unwrap_or(0);
            let sessions = sqlx::query(PURGE_SESSIONS)
                .bind(big(now))
                .execute(&self.pool)
                .await
                .map(|done| done.rows_affected())
                .unwrap_or(0);
            usize::try_from(denied + sessions).unwrap_or(usize::MAX)
        })
    }
}
