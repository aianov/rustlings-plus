fn invitations() -> &'static str {
    "SELECT u.username, inv.username AS invited_by
     FROM users u
     LEFT JOIN users inv ON inv.id = u.invited_by
     ORDER BY u.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn names_of_inviters() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(invitations()).await;
        assert_eq!(t.columns, ["username", "invited_by"], "нужны столбцы username и invited_by");
        assert_eq!(
            t.rows,
            [["alice", "NULL"], ["bob", "alice"], ["carol", "alice"], ["dave", "bob"], ["eve", "carol"], ["frank", "alice"], ["grace", "NULL"]],
            "вместо числа должен быть ник пригласившего, а alice и grace остаются с NULL:\n{t}"
        );
    }
}
