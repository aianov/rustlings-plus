use contracts::{Cause, Fact, UserId};
use db_engine::DbError;

use super::pg::{PgUsers, id};

const LOCK_PAIR: &str = "SELECT id FROM users WHERE id = ANY($1) ORDER BY id FOR UPDATE";
const INSERT_FOLLOW: &str =
    "INSERT INTO follows (follower, followee) VALUES ($1, $2) ON CONFLICT DO NOTHING";
const COUNT_FOLLOW: &str = "UPDATE users SET following_count = following_count + (id = $1)::int, followers_count = followers_count + (id = $2)::int WHERE id IN ($1, $2)";

impl PgUsers {
    pub(super) async fn follow_counted(
        &self,
        who: UserId,
        whom: UserId,
        fact: Fact,
        cause: &Cause,
    ) -> Result<bool, DbError> {
        let mut pair = [id(who), id(whom)];
        pair.sort_unstable();
        let mut tx = self.pool.begin().await?;
        sqlx::query(LOCK_PAIR)
            .bind(&pair[..])
            .execute(&mut *tx)
            .await?;
        let added = sqlx::query(INSERT_FOLLOW)
            .bind(id(who))
            .bind(id(whom))
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if added {
            sqlx::query(COUNT_FOLLOW)
                .bind(id(who))
                .bind(id(whom))
                .execute(&mut *tx)
                .await?;
        }
        self.commit(tx, added.then_some(fact), cause).await?;
        Ok(added)
    }
}
