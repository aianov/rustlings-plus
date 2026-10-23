// Хочется увидеть MVCC своими глазами. Функция `peek_uncommitted` в таблице posts (уже две строки)
// открывает транзакцию, вставляет пост с body 'peek' и до COMMIT возвращает три значения:
// сколько постов видно изнутри этой транзакции, сколько видит в тот же момент другое соединение
// (запрос прямо на pool) и совпадает ли системный столбец xmin новой строки с номером текущей
// транзакции (`txid_current()`, младшие 32 бита). В конце транзакция откатывается.
// TODO: Допиши функцию.

use sqlx::PgPool;

async fn peek_uncommitted(pool: &PgPool) -> Result<(i64, i64, bool), sqlx::Error> {
    Ok((0, 0, false))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn each_connection_sees_its_own_versions() {
        let db = TestDb::with_sql("CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, body text NOT NULL); INSERT INTO posts (body) VALUES ('a'), ('b');").await;
        let (inside, outside, mine) = peek_uncommitted(&db.pool).await.expect("запросы должны выполниться");
        assert_eq!(inside, 3, "своя транзакция видит свою вставку: две старые строки и новая");
        assert_eq!(outside, 2, "другое соединение незакоммиченную строку не видит");
        assert!(mine, "xmin новой строки это номер той транзакции, что её создала");
        assert_eq!(db.table("SELECT * FROM posts").await.len(), 2, "после отката в таблице снова две строки");
    }
}
