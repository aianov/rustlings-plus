fn never_invited_anyone() -> &'static str {
    "SELECT username FROM users u
     WHERE NOT EXISTS (SELECT 1 FROM users i WHERE i.invited_by = u.id)
     ORDER BY u.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn finds_users_who_invited_nobody() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(never_invited_anyone()).await;
        assert_eq!(t.columns, ["username"], "нужен один столбец username");
        assert_eq!(
            t.rows,
            [["dave"], ["eve"], ["frank"], ["grace"]],
            "никого не пригласили dave, eve, frank и grace; пустой ответ значит, что NULL в подзапросе съел весь результат:\n{t}"
        );
    }

    #[tokio::test]
    async fn still_works_after_changes() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO users (username, invited_by) VALUES ('heidi', 4)").await;
        let t = db.table(never_invited_anyone()).await;
        assert_eq!(
            t.rows,
            [["eve"], ["frank"], ["grace"], ["heidi"]],
            "dave теперь пригласил heidi и выпал из списка:\n{t}"
        );
    }
}
