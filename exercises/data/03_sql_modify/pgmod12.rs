// Две операции, где данные берутся из другой таблицы.
// 1. Раз в неделю очки из таблицы weekly_scores (user_id, points) добавляются к users.score
//    соответствующего пользователя. Остальные пользователи не меняются. В weekly_scores
//    бывают строки с несуществующим user_id: они игнорируются.
// 2. Забаненных не должны видеть: удали из follows все подписки НА забаненных пользователей
//    (followee_id, чей статус banned). Подписки самих забаненных на других остаются.
// TODO: Напиши оба запроса.

fn apply_weekly() -> &'static str {
    "SELECT 1"
}

fn drop_follows_of_banned() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn scores_are_added_only_to_matching_users() {
        let db = TestDb::with_sql(SEED).await;
        db.run(apply_weekly()).await;
        let t = db.table("SELECT username, score FROM users ORDER BY id").await;
        assert_eq!(
            t.rows,
            [["alice", "110"], ["bob", "55"], ["carol", "0"], ["dave", "20"], ["eve", "7"], ["frank", "0"]],
            "очки добавляются alice, bob и eve; строка с user_id 99 игнорируется:\n{t}"
        );
    }

    #[tokio::test]
    async fn follows_of_banned_are_removed() {
        let db = TestDb::with_sql(SEED).await;
        db.run(drop_follows_of_banned()).await;
        let t = db.table("SELECT follower_id, followee_id FROM follows ORDER BY follower_id, followee_id").await;
        assert_eq!(t.rows, [["1", "2"], ["2", "1"]], "должны остаться подписки не на carol:\n{t}");
    }
}
