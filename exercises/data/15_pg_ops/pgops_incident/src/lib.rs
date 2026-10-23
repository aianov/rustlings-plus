use sqlx::PgPool;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diagnosis {
    pub waiting: Vec<String>,
    pub blockers: Vec<String>,
    pub idle_in_tx: Vec<String>,
    pub horizon_holders: Vec<String>,
}

pub async fn diagnose(pool: &PgPool, app_prefix: &str) -> Result<Diagnosis, sqlx::Error> {
    let _ = (pool, app_prefix);
    todo!("четыре списка из pg_stat_activity")
}

pub async fn resolve(pool: &PgPool, app_prefix: &str) -> Result<Vec<String>, sqlx::Error> {
    let _ = (pool, app_prefix);
    todo!("закрыть держателей замков и горизонта")
}

pub async fn vacuum_bloated(pool: &PgPool, min_dead_pct: i32) -> Result<Vec<String>, sqlx::Error> {
    let _ = (pool, min_dead_pct);
    todo!("найти раздутые таблицы и выполнить VACUUM")
}

pub async fn harden_role(pool: &PgPool, role: &str, password: &str) -> Result<(), sqlx::Error> {
    let _ = (pool, role, password);
    todo!("роль с таймаутами и минимальными правами")
}
