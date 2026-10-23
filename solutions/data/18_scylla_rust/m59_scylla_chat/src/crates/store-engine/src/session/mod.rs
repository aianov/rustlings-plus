use std::sync::Arc;

use scylla::prepared_statement::PreparedStatement;
use scylla::statement::Consistency;
use scylla::{Session, SessionBuilder};

use crate::config::{StoreConfig, good_keyspace};
use crate::error::StoreError;

pub async fn connect(config: &StoreConfig) -> Result<Arc<Session>, StoreError> {
    if !good_keyspace(&config.keyspace) {
        return Err(StoreError::BadKeyspace(config.keyspace.clone()));
    }
    let session = SessionBuilder::new()
        .known_node(&config.uri)
        .connection_timeout(config.connect_timeout)
        .build()
        .await?;
    let create = format!(
        "CREATE KEYSPACE IF NOT EXISTS {} WITH replication = {{'class': 'SimpleStrategy', 'replication_factor': 1}}",
        config.keyspace
    );
    session.query(create, ()).await?;
    session.await_schema_agreement().await?;
    session.use_keyspace(&config.keyspace, false).await?;
    Ok(Arc::new(session))
}

pub async fn drop_keyspace(config: &StoreConfig) -> Result<(), StoreError> {
    if !good_keyspace(&config.keyspace) {
        return Err(StoreError::BadKeyspace(config.keyspace.clone()));
    }
    let session = SessionBuilder::new()
        .known_node(&config.uri)
        .connection_timeout(config.connect_timeout)
        .build()
        .await?;
    session
        .query(format!("DROP KEYSPACE IF EXISTS {}", config.keyspace), ())
        .await?;
    session.await_schema_agreement().await?;
    Ok(())
}

pub async fn prepared(
    session: &Session,
    cql: &str,
    idempotent: bool,
) -> Result<PreparedStatement, StoreError> {
    let mut statement = session.prepare(cql).await?;
    statement.set_consistency(Consistency::LocalQuorum);
    statement.set_is_idempotent(idempotent);
    Ok(statement)
}
