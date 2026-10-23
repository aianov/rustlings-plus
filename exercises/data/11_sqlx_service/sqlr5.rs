// Под нагрузкой Postgres иногда отвечает «попробуй ещё раз»: 40001 (не удалось сериализовать
// доступ) и 40P01 (взаимная блокировка). Транзакцию в таких случаях надо повторить целиком, но не бесконечно
// и не при любой ошибке: нарушение уникальности повтором не вылечить.
// TODO: Напиши `retry_tx`: она вызывает `op` до `max_attempts` раз. Успех возвращает сразу. Ошибки
// 40001 и 40P01 повторяют, остальные ошибки отдаются сразу. Когда попытки кончились, верни последнюю ошибку.

use std::future::Future;

async fn retry_tx<T, F, Fut>(max_attempts: u32, mut op: F) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>, {
    op().await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Debug)]
    struct Fake(&'static str);

    impl std::fmt::Display for Fake {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "fake {}", self.0)
        }
    }

    impl std::error::Error for Fake {}

    impl sqlx::error::DatabaseError for Fake {
        fn message(&self) -> &str {
            "fake"
        }
        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            Some(self.0.into())
        }
        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }
        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
    }

    fn db_error(code: &'static str) -> sqlx::Error {
        sqlx::Error::Database(Box::new(Fake(code)))
    }

    fn failing(codes: Vec<&'static str>, calls: &Arc<AtomicU32>) -> impl FnMut() -> std::future::Ready<Result<u32, sqlx::Error>> {
        let calls = calls.clone();
        move || {
            let n = calls.fetch_add(1, Ordering::SeqCst) as usize;
            std::future::ready(match codes.get(n) {
                Some(code) => Err(db_error(code)),
                None => Ok(42),
            })
        }
    }

    #[tokio::test]
    async fn retries_serialization_failure_and_deadlock() {
        let calls = Arc::new(AtomicU32::new(0));
        let res = retry_tx(5, failing(vec!["40001", "40P01"], &calls)).await;
        assert_eq!(res.expect("после двух повторяемых ошибок третья попытка удалась"), 42, "должно вернуться значение удачной попытки");
        assert_eq!(calls.load(Ordering::SeqCst), 3, "операцию должны были вызвать три раза: две неудачи и успех");
    }

    #[tokio::test]
    async fn first_success_is_not_repeated() {
        let calls = Arc::new(AtomicU32::new(0));
        let res = retry_tx(5, failing(vec![], &calls)).await;
        assert_eq!(res.expect("успех с первого раза"), 42, "значение должно дойти до вызывающего");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "успешную операцию повторять нельзя");
    }

    #[tokio::test]
    async fn other_errors_are_returned_immediately() {
        let calls = Arc::new(AtomicU32::new(0));
        let res = retry_tx(5, failing(vec!["23505", "40001"], &calls)).await;
        match res {
            Err(sqlx::Error::Database(e)) => assert_eq!(e.code().as_deref(), Some("23505"), "вернуться должна именно ошибка уникальности"),
            other => panic!("ожидалась ошибка базы 23505, а пришло {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "нарушение уникальности повтором не лечится, повторять нельзя");
        let calls = Arc::new(AtomicU32::new(0));
        let res = retry_tx(5, || {
            calls.fetch_add(1, Ordering::SeqCst);
            std::future::ready(Err::<u32, _>(sqlx::Error::RowNotFound))
        })
        .await;
        assert!(matches!(res, Err(sqlx::Error::RowNotFound)), "ошибка не из базы должна дойти как есть");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "не-базовую ошибку тоже не повторяем");
    }

    #[tokio::test]
    async fn gives_up_after_the_limit_with_the_last_error() {
        let calls = Arc::new(AtomicU32::new(0));
        let res = retry_tx(3, failing(vec!["40001", "40001", "40P01", "40001"], &calls)).await;
        match res {
            Err(sqlx::Error::Database(e)) => assert_eq!(e.code().as_deref(), Some("40P01"), "после трёх попыток должна вернуться последняя ошибка, 40P01"),
            other => panic!("ожидалась ошибка после исчерпания попыток, а пришло {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3, "ровно max_attempts вызовов, не больше и не меньше");
    }
}
