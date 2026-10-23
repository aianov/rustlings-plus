// Скрипт добавляет метку 'rust' в таблицу labels и должен безопасно запускаться много раз:
// метка не дублируется, ошибки при повторе нет. Он написан по рецепту «ON CONFLICT по имени», но
// падает уже на первом запуске. Разберись, чего требует база для такой записи, и дополни
// скрипт (его могут запускать сколько угодно раз подряд, в том числе после успешного).
// Скрипт может состоять из нескольких операторов через `;`.
// TODO: Почини скрипт.

fn add_label() -> &'static str {
    "INSERT INTO labels (name) VALUES ('rust') ON CONFLICT (name) DO NOTHING"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn script_is_repeatable() {
        let db = TestDb::with_sql(SEED).await;
        db.run(add_label()).await;
        db.run(add_label()).await;
        db.run(add_label()).await;
        let t = db.table("SELECT name FROM labels").await;
        assert_eq!(t.rows, [["rust"]], "после трёх запусков метка должна быть одна:\n{t}");
    }

    #[tokio::test]
    async fn uniqueness_is_enforced_by_database() {
        let db = TestDb::with_sql(SEED).await;
        db.run(add_label()).await;
        let err = db.fails("INSERT INTO labels (name) VALUES ('rust')").await;
        assert!(err.is_some_and(|e| e.code == "23505"), "после скрипта база сама должна отвергать дубли меток");
    }
}
