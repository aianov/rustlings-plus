use sqlx::PgPool;

async fn bump(pool: &PgPool, post_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE posts SET views = views + 1 WHERE id = $1").bind(post_id).execute(pool).await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn twelve_parallel_bumps_make_twelve() {
        let db = TestDb::with_sql("CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, views integer NOT NULL DEFAULT 0); INSERT INTO posts DEFAULT VALUES;").await;
        let mut handles = Vec::new();
        for _ in 0..12 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { bump(&pool, 1).await }));
        }
        for h in handles {
            h.await.expect("задача упала").expect("bump вернул ошибку");
        }
        let views = db.table("SELECT views FROM posts WHERE id = 1").await.cell(0, 0).to_string();
        assert_eq!(views, "12", "потеряно обновлений: {}. Два показа прочитали одно значение и записали одно и то же (потерянное обновление)", 12 - views.parse::<i32>().unwrap_or(0));
    }
}
