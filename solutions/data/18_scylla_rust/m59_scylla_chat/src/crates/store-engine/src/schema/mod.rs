use scylla::Session;

use crate::error::StoreError;

pub type Schema = (&'static str, &'static str);

pub fn statements(cql: &str) -> Vec<String> {
    cql.split(';')
        .map(|statement| {
            statement
                .lines()
                .filter(|line| !line.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        })
        .filter(|statement| !statement.is_empty())
        .collect()
}

pub async fn apply(session: &Session, files: &[Schema]) -> Result<usize, StoreError> {
    let mut applied = 0;
    for (name, cql) in files {
        for statement in statements(cql) {
            session
                .query(statement, ())
                .await
                .map_err(|error| StoreError::Schema(format!("{name}: {error}")))?;
        }
        applied += 1;
    }
    session.await_schema_agreement().await?;
    Ok(applied)
}
