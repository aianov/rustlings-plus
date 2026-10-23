// Импорт тысяч новых пользователей: сейчас на каждого уходит отдельный INSERT, то есть тысяча
// походов в базу и тысяча проверок триггеров. Нужно вставить всех одной командой. Ники даны
// списком, кармы списком такой же длины. Занятые ники пропускаются без ошибки и без
// перезаписи. Вернуть надо, сколько пользователей добавлено на самом деле.
// TODO: Перепиши на одну команду, передав два массива.

use sqlx::PgPool;

async fn import_users(pool: &PgPool, names: &[String], karmas: &[i32]) -> Result<u64, sqlx::Error> {
    let mut added = 0;
    for (name, karma) in names.iter().zip(karmas) {
        added += sqlx::query("INSERT INTO users (username, karma) VALUES ($1, $2) ON CONFLICT (username) DO NOTHING")
            .bind(name)
            .bind(karma)
            .execute(pool)
            .await?
            .rows_affected();
    }
    Ok(added)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    const COUNTER: &str = "
        CREATE TABLE stmt_log (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
        CREATE FUNCTION log_stmt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO stmt_log DEFAULT VALUES; RETURN NULL; END $$;
        CREATE TRIGGER users_stmt AFTER INSERT ON users FOR EACH STATEMENT EXECUTE FUNCTION log_stmt();
    ";

    fn batch(n: usize) -> (Vec<String>, Vec<i32>) {
        let mut names: Vec<String> = (0..n).map(|i| format!("u{i}")).collect();
        let mut karmas: Vec<i32> = (0..n).map(|i| (i % 5) as i32).collect();
        names.push("anna".to_string());
        karmas.push(99);
        names.push("boris".to_string());
        karmas.push(99);
        (names, karmas)
    }

    #[tokio::test]
    async fn everything_goes_in_one_statement() {
        let db = TestDb::with_sql(SEED).await;
        db.run(COUNTER).await;
        let (names, karmas) = batch(1000);
        let added = import_users(&db.pool, &names, &karmas).await.expect("массовый импорт");
        assert_eq!(added, 1000, "новых 1000, а anna и boris заняты и добавляться не должны");
        let t = db.table("SELECT count(*) FROM stmt_log").await;
        assert_eq!(t.cell(0, 0), "1", "для тысячи пользователей должна быть выполнена одна команда INSERT, а их было {}", t.cell(0, 0));
        let t = db.table("SELECT username, karma FROM users WHERE username IN ('u7', 'anna') ORDER BY username").await;
        assert_eq!(t.rows, vec![vec!["anna", "50"], vec!["u7", "2"]], "карма должна соответствовать нику по позиции, занятый ник не перезаписывается:\n{t}");
    }

    #[tokio::test]
    async fn empty_input_is_fine() {
        let db = TestDb::with_sql(SEED).await;
        let added = import_users(&db.pool, &[], &[]).await.expect("пустой список");
        assert_eq!(added, 0, "из пустого списка ничего не добавляется");
    }
}
