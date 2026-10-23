use std::sync::Arc;

use crate::common::{Config, Error, Result};
use crate::repository::{MIGRATIONS, PgAccounts, Stores};

pub async fn open(config: &Config) -> Result<Stores> {
    match (&config.database, &config.stores) {
        (Some(database), _) => {
            let pool = db_engine::prepare(database, &MIGRATIONS)
                .await
                .map_err(Error::Db)?;
            Ok(Stores::postgres(Arc::new(PgAccounts::new(
                pool,
                config.lifetimes.leeway,
            ))))
        }
        (None, Some(stores)) => Ok(stores.clone()),
        (None, None) => Err(Error::NoStorage),
    }
}
