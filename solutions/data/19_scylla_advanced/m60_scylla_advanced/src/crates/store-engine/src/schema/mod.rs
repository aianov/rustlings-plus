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

pub fn already_there(statement: &str, error: &str) -> bool {
    statement
        .trim_start()
        .to_uppercase()
        .starts_with("ALTER TABLE")
        && (error.contains("conflicts with an existing column") || error.contains("already exist"))
}

pub async fn apply(session: &Session, files: &[Schema]) -> Result<usize, StoreError> {
    let mut applied = 0;
    for (name, cql) in files {
        for statement in statements(cql) {
            match session.query(statement.as_str(), ()).await {
                Ok(..) => {}
                Err(error) if already_there(&statement, &error.to_string()) => {}
                Err(error) => return Err(StoreError::Schema(format!("{name}: {error}"))),
            }
        }
        applied += 1;
    }
    session.await_schema_agreement().await?;
    Ok(applied)
}
