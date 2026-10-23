// В соцсети можно подарить часть своей кармы другому пользователю. Правила простые: карма не
// уходит в минус (не хватает, значит ничего не происходит и ответ false), а если получателя
// нет, то у отправителя ничего не должно списаться. Сейчас каждый UPDATE живёт сам по себе.
// TODO: Сделай перевод так, чтобы карма не могла потеряться на полпути.

use sqlx::PgPool;

async fn tip(pool: &PgPool, from: i64, to: i64, amount: i32) -> Result<bool, sqlx::Error> {
    let debit = sqlx::query("UPDATE users SET karma = karma - $1 WHERE id = $2 AND karma >= $1")
        .bind(amount)
        .bind(from)
        .execute(pool)
        .await?;
    if debit.rows_affected() == 0 {
        return Ok(false);
    }
    let credit = sqlx::query("UPDATE users SET karma = karma + $1 WHERE id = $2")
        .bind(amount)
        .bind(to)
        .execute(pool)
        .await?;
    Ok(credit.rows_affected() == 1)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn karma(db: &TestDb, id: i64) -> String {
        db.table(&format!("SELECT karma FROM users WHERE id = {id}")).await.cell(0, 0).to_string()
    }

    #[tokio::test]
    async fn karma_moves_between_users() {
        let db = TestDb::with_sql(SEED).await;
        let done = tip(&db.pool, 1, 2, 20).await.expect("перевод между живыми пользователями должен пройти");
        assert!(done, "у anna 50 кармы, перевод 20 должен получиться");
        assert_eq!(karma(&db, 1).await, "30", "у отправителя должно остаться 30");
        assert_eq!(karma(&db, 2).await, "25", "получатель должен получить 20 и стать 25");
    }

    #[tokio::test]
    async fn not_enough_karma_changes_nothing() {
        let db = TestDb::with_sql(SEED).await;
        let done = tip(&db.pool, 3, 1, 5).await.expect("нехватка кармы не ошибка, а ответ false");
        assert!(!done, "у clara 0 кармы, дарить 5 нечем");
        assert_eq!(karma(&db, 3).await, "0", "у отправителя ничего не должно списаться");
        assert_eq!(karma(&db, 1).await, "50", "получатель ничего не должен получить");
        let all = tip(&db.pool, 4, 3, 20).await.expect("можно отдать всю карму");
        assert!(all, "ровно столько, сколько есть, отдать можно");
        assert_eq!(karma(&db, 4).await, "0", "у dmitry карма должна обнулиться");
    }

    #[tokio::test]
    async fn missing_recipient_keeps_the_karma() {
        let db = TestDb::with_sql(SEED).await;
        let done = tip(&db.pool, 1, 999, 10).await.expect("несуществующий получатель не ошибка базы");
        assert!(!done, "получателя 999 нет, перевод не состоялся");
        assert_eq!(
            karma(&db, 1).await,
            "50",
            "карма отправителя списалась, а получателя нет: деньги, то есть карма, пропали. Списание и зачисление должны быть одной транзакцией"
        );
    }
}
