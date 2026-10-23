use contracts::{Cause, Fact, UserDeleted, UserId};
use db_engine::DbError;
use service_runtime::StorageError;

use super::pg::{
    ERASE_FOLLOWS, ERASE_MUTES, ERASE_USER, HIDE_USER, PgUsers, SHOW_USER, id, storage,
};
use super::{BoxFuture, LifecycleRepository};

impl PgUsers {
    async fn flip(
        &self,
        query: &str,
        user: UserId,
        fact: Option<Fact>,
        cause: &Cause,
        erase: bool,
    ) -> Result<bool, DbError> {
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query(query)
            .bind(id(user))
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if changed && erase {
            sqlx::query(ERASE_FOLLOWS)
                .bind(id(user))
                .execute(&mut *tx)
                .await?;
            sqlx::query(ERASE_MUTES)
                .bind(id(user))
                .execute(&mut *tx)
                .await?;
        }
        self.commit(tx, fact.filter(|_| changed), cause).await?;
        Ok(changed)
    }
}

impl LifecycleRepository for PgUsers {
    fn hide(&self, user: UserId, cause: &Cause) -> BoxFuture<'_, Result<bool, StorageError>> {
        let cause = cause.clone();
        Box::pin(async move {
            self.flip(HIDE_USER, user, None, &cause, false)
                .await
                .map_err(storage)
        })
    }

    fn show(&self, user: UserId, cause: &Cause) -> BoxFuture<'_, Result<bool, StorageError>> {
        let cause = cause.clone();
        Box::pin(async move {
            self.flip(SHOW_USER, user, None, &cause, false)
                .await
                .map_err(storage)
        })
    }

    fn erase(&self, user: UserId, cause: &Cause) -> BoxFuture<'_, Result<bool, StorageError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let fact = Fact::UserDeleted(UserDeleted { user_id: user });
            self.flip(ERASE_USER, user, Some(fact), &cause, true)
                .await
                .map_err(storage)
        })
    }
}
