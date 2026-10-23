// Лента собрана из id постов, а теперь нужно показать авторов. Для списка из 50 id нельзя делать
// 50 запросов, нужен один: «все пользователи, чей id входит в список». Список приходит срезом,
// и он может быть пустым. Результат идёт в том же порядке, что и список; пользователь, которого
// нет в базе, пропускается, а повторяющийся id даёт одну запись.
// TODO: Реализуй функцию одним запросом.

use sqlx::PgPool;

#[derive(Debug, sqlx::FromRow)]
struct Brief {
    id: i64,
    username: String,
}

async fn users_by_ids(pool: &PgPool, ids: &[i64]) -> Result<Vec<Brief>, sqlx::Error> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    fn names(list: &[Brief]) -> Vec<&str> {
        list.iter().map(|b| b.username.as_str()).collect()
    }

    #[tokio::test]
    async fn keeps_requested_order() {
        let db = TestDb::with_sql(SEED).await;
        let got = users_by_ids(&db.pool, &[3, 1, 5]).await.expect("запрос должен выполниться");
        assert_eq!(names(&got), ["carol", "alice", "eve"], "порядок как в списке id, а не по возрастанию");
    }

    #[tokio::test]
    async fn empty_list_is_fine() {
        let db = TestDb::with_sql(SEED).await;
        let got = users_by_ids(&db.pool, &[]).await.expect("пустой список это не ошибка");
        assert!(got.is_empty(), "по пустому списку ничего не находится: {got:?}");
    }

    #[tokio::test]
    async fn missing_and_repeated_ids() {
        let db = TestDb::with_sql(SEED).await;
        let got = users_by_ids(&db.pool, &[2, 99, 2]).await.unwrap();
        assert_eq!(names(&got), ["bob"], "99 нет в базе, а bob должен выйти один раз");
        assert_eq!(got[0].id, 2, "id bob");
    }
}
