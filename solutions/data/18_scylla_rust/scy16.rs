use scylla::transport::errors::QueryError;
use scylla::Session;
use std::error::Error;
use std::sync::Arc;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Clone)]
struct Db {
    session: Arc<Session>,
    keyspace: String,
}

impl Db {
    fn new(session: Session, keyspace: &str) -> Db {
        Db { session: Arc::new(session), keyspace: keyspace.to_string() }
    }

    async fn migrate(&self) -> Result<(), QueryError> {
        self.session
            .query(format!("CREATE TABLE IF NOT EXISTS {}.notes (id int PRIMARY KEY, text text)", self.keyspace), ())
            .await?;
        self.session.await_schema_agreement().await?;
        Ok(())
    }

    async fn add(&self, id: i32, text: &str) -> Result<(), QueryError> {
        self.session.query(format!("INSERT INTO {}.notes (id, text) VALUES (?, ?)", self.keyspace), (id, text)).await?;
        Ok(())
    }

    async fn count(&self) -> Result<i64, BoxError> {
        let res = self.session.query(format!("SELECT COUNT(*) FROM {}.notes", self.keyspace), ()).await?;
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
