use sqlx::PgPool;

use crate::{MigrateError, Migration, Options, Report, Status};

pub struct Migrator {
    pool: PgPool,
    options: Options,
}

impl Migrator {
    pub fn new(pool: PgPool, options: Options) -> Migrator {
        Migrator { pool, options }
    }

    pub async fn run(&self, migrations: &[Migration]) -> Result<Report, MigrateError> {
        let _ = (&self.pool, &self.options, migrations);
        todo!()
    }

    pub async fn status(&self, migrations: &[Migration]) -> Result<Vec<Status>, MigrateError> {
        let _ = (&self.pool, migrations);
        todo!()
    }
}
