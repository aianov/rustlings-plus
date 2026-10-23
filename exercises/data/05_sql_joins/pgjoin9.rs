// «Друзья» в нашей сети это взаимные подписки: A подписан на B, и B подписан на A. Нужен список
// таких пар с никами (столбцы friend_a, friend_b), каждая пара один раз, первым в паре идёт тот, у кого
// id меньше, список упорядочен по friend_a, затем friend_b.
// TODO: Напиши запрос по таблице follows (follower_id, followee_id).

fn friends() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn each_pair_once() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(friends()).await;
        assert_eq!(t.columns, ["friend_a", "friend_b"], "нужны столбцы friend_a и friend_b");
        assert_eq!(
            t.rows,
            [["alice", "bob"], ["alice", "carol"], ["dave", "eve"]],
            "взаимных пар три; каждая должна быть один раз, а не в обе стороны:\n{t}"
        );
    }
}
