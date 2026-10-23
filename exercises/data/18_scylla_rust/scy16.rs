// Сервис держит одну сессию на всё приложение и раздаёт её обработчикам. Сейчас Db не клонируется, и
// каждый обработчик открывал бы свою сессию (дорого). Кроме того, сессия не переключена на keyspace (сервисы
// делят кластер), поэтому запросы без имени keyspace не работают, а после создания таблицы следующий запрос
// иногда не видит её: схема ещё не разошлась по узлам. Почини Db: одна сессия, разделяемая между задачами,
// имя keyspace в каждом запросе, ожидание согласования схемы после DDL.

use scylla::transport::errors::QueryError;
use scylla::Session;
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

struct Db {
    session: Session,
    keyspace: String,
}

impl Db {
    fn new(session: Session, keyspace: &str) -> Db {
        Db { session, keyspace: keyspace.to_string() }
    }

    async fn migrate(&self) -> Result<(), QueryError> {
        let _ = &self.keyspace;
        self.session.query("CREATE TABLE IF NOT EXISTS notes (id int PRIMARY KEY, text text)", ()).await?;
        Ok(())
    }

    async fn add(&self, id: i32, text: &str) -> Result<(), QueryError> {
        self.session.query("INSERT INTO notes (id, text) VALUES (?, ?)", (id, text)).await?;
        Ok(())
    }

    async fn count(&self) -> Result<i64, BoxError> {
        let res = self.session.query("SELECT COUNT(*) FROM notes", ()).await?;
        Ok(res.first_row_typed::<(i64,)>()?.0)
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SOURCE: &str = include_str!("scy16.rs");

    #[tokio::test]
    async fn one_session_many_tasks() {
        let ks = TestKs::new().await;
        let db = Db::new(course_support::scylla::session().await, ks.name());
        db.migrate().await.expect("создание таблицы");
        let mut tasks = Vec::new();
        for i in 0..10 {
            let db = db.clone();
            tasks.push(tokio::spawn(async move { db.add(i, "заметка").await }));
        }
        for t in tasks {
            t.await.expect("задача").expect("запись");
        }
        assert_eq!(db.count().await.expect("подсчёт"), 10, "все десять задач записали через одну сессию");
    }

    #[test]
    fn waits_for_schema_agreement() {
        let code = SOURCE.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(code.contains("await_schema_agreement"), "после DDL дождись согласования схемы");
    }
}
