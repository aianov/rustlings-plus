use std::env;

use db_engine::{DB_URL_ENV, DEFAULT_DB_URL, DbConfig, Migration, connect, migrate};

const SERVICES: [(&str, &str, &[Migration]); 3] = [
    ("auth", "AUTH", &auth_service::MIGRATIONS),
    ("users", "USERS", &user_service::MIGRATIONS),
    ("posts", "POSTS", &post_service::MIGRATIONS),
];

pub fn run() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("рантайм не поднялся: {error}"))?;
    runtime.block_on(async {
        for (schema, prefix, migrations) in SERVICES {
            let url = env::var(format!("{prefix}_DATABASE_URL"))
                .or_else(|_| env::var(DB_URL_ENV))
                .unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
            let config = DbConfig::new(&url, schema);
            let pool = connect(&config)
                .await
                .map_err(|error| format!("{schema}: {error}"))?;
            let ran = migrate(&pool, migrations)
                .await
                .map_err(|error| format!("{schema}: {error}"))?;
            match ran.is_empty() {
                true => eprintln!("{schema}: схема уже на версии {}", migrations.len() - 1),
                false => eprintln!("{schema}: применены {}", ran.join(", ")),
            }
            pool.close().await;
        }
        Ok(())
    })
}
