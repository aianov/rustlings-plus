use std::sync::Arc;

use journal::Journal;

use crate::common::{Account, Config, Error, Result};
use crate::repository::{
    AccountRepository, MIGRATIONS, MemoryAccounts, MemoryOneTime, MemorySessions,
    OneTimeRepository, PgAccounts, SessionRepository,
};

pub type Stores = (
    Arc<dyn AccountRepository>,
    Arc<dyn SessionRepository>,
    Arc<dyn OneTimeRepository>,
);

pub async fn open(
    config: &Config,
    journal: Journal<Account>,
    accounts: Vec<Account>,
) -> Result<Stores> {
    match &config.database {
        Some(database) => {
            let pool = db_engine::connect(database).await.map_err(Error::Db)?;
            db_engine::apply(&pool, &MIGRATIONS)
                .await
                .map_err(Error::Db)?;
            let pg = Arc::new(PgAccounts::new(pool, config.lifetimes.leeway));
            Ok((pg.clone(), pg.clone(), pg))
        }
        None => Ok((
            Arc::new(MemoryAccounts::restore(journal, accounts)),
            Arc::new(MemorySessions::new(config.lifetimes.leeway)),
            Arc::new(MemoryOneTime::default()),
        )),
    }
}
