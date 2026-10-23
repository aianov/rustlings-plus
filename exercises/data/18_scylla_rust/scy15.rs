// Store-слой не должен выпускать наружу ошибки драйвера: сервису нужны свои варианты, по которым ясно,
// что делать. Неверный запрос это баг (Invalid с текстом), недоступные реплики, перегрузка и обрыв соединения
// это Unavailable (можно повторить позже), истёкшие сроки это Timeout, остальное Other с текстом.
// Отсутствие профиля это NotFound.
// TODO: `map_error` переводит QueryError, `nickname` читает ник пользователя.

use scylla::transport::errors::{DbError, QueryError};
use scylla::Session;

#[derive(Debug, PartialEq)]
enum StoreError {
    NotFound,
    Invalid(String),
    Unavailable,
    Timeout,
    Other(String),
}

fn map_error(e: QueryError) -> StoreError {
    let _ = DbError::Invalid;
    StoreError::Other(e.to_string())
}

async fn nickname(session: &Session, user: i32) -> Result<String, StoreError> {
    let _ = (session, user);
    todo!("чтение ника с переводом ошибок")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    use scylla::statement::Consistency;
    use std::sync::Arc;

    #[test]
    fn driver_errors_become_store_errors() {
        let unavailable = QueryError::DbError(DbError::Unavailable { consistency: Consistency::Quorum, required: 2, alive: 1 }, "мало реплик".into());
        assert_eq!(map_error(unavailable), StoreError::Unavailable);
        assert_eq!(map_error(QueryError::DbError(DbError::Overloaded, "перегруз".into())), StoreError::Unavailable);
        assert_eq!(map_error(QueryError::IoError(Arc::new(std::io::Error::other("обрыв")))), StoreError::Unavailable, "обрыв соединения это временная недоступность");
        assert_eq!(map_error(QueryError::TimeoutError), StoreError::Timeout);
        assert_eq!(map_error(QueryError::RequestTimeout("долго".into())), StoreError::Timeout);
        assert_eq!(map_error(QueryError::DbError(DbError::Invalid, "нет столбца".into())), StoreError::Invalid("нет столбца".into()));
        assert_eq!(map_error(QueryError::DbError(DbError::SyntaxError, "синтаксис".into())), StoreError::Invalid("синтаксис".into()));
        assert!(matches!(map_error(QueryError::DbError(DbError::Unauthorized, "нельзя".into())), StoreError::Other(_)), "остальное это Other");
    }

    #[tokio::test]
    async fn nickname_and_not_found() {
        let ks = TestKs::with_cql("CREATE TABLE profiles (user_id int PRIMARY KEY, nickname text)").await;
        ks.run("INSERT INTO profiles (user_id, nickname) VALUES (7, 'kate')").await;
        assert_eq!(nickname(&ks.session, 7).await, Ok("kate".to_string()));
        assert_eq!(nickname(&ks.session, 8).await, Err(StoreError::NotFound), "нет профиля это NotFound");
        ks.run("DROP TABLE profiles").await;
        assert!(matches!(nickname(&ks.session, 7).await, Err(StoreError::Invalid(_))), "запрос к несуществующей таблице это Invalid");
    }
}
