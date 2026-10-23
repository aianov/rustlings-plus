use sqlx::PgPool;

use crate::error::DbError;

pub type Migration = (&'static str, &'static str);

pub async fn apply(pool: &PgPool, migrations: &[Migration]) -> Result<usize, DbError> {
    let mut applied = 0;
    for (name, sql) in migrations {
        sqlx::raw_sql(sql)
            .execute(pool)
            .await
            .map_err(|error| match DbError::from(error) {
                DbError::Query(why) => DbError::Query(format!("{name}: {why}")),
                other => other,
            })?;
        applied += 1;
    }
    Ok(applied)
}
