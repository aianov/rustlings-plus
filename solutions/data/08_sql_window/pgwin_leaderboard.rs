fn leaderboard() -> &'static str {
    "WITH r AS (
         SELECT author_id, week, score,
                rank() OVER (PARTITION BY week ORDER BY score DESC) AS place,
                max(score) OVER (PARTITION BY week) AS top
         FROM weekly_scores
         WHERE week IN ($1, $1 - 1)
     )
     SELECT u.username, cur.place, cur.top - cur.score AS behind,
            prev.place AS prev_place, prev.place - cur.place AS moved
     FROM r cur
     JOIN users u ON u.id = cur.author_id
     LEFT JOIN r prev ON prev.author_id = cur.author_id AND prev.week = $1 - 1
     WHERE cur.week = $1
     ORDER BY cur.place, u.username"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn board(db: &TestDb, week: i32) -> Vec<Vec<String>> {
        let sql = format!("SELECT q.username::text, q.place::text, q.behind::text, COALESCE(q.prev_place::text, 'NULL'), COALESCE(q.moved::text, 'NULL') FROM ({}) AS q", leaderboard());
        let rows = sqlx::query_as::<_, (String, String, String, String, String)>(&sql)
            .bind(week)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("leaderboard({week}): {e}"));
        rows.into_iter().map(|r| vec![r.0, r.1, r.2, r.3, r.4]).collect()
    }

    fn line(cells: [&str; 5]) -> Vec<String> {
        cells.iter().map(|c| c.to_string()).collect()
    }

    #[tokio::test]
    async fn places_gaps_and_movement() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            board(&db, 41).await,
            [
                line(["alice", "1", "0", "3", "2"]),
                line(["bob", "1", "0", "1", "0"]),
                line(["carol", "3", "20", "1", "-2"]),
                line(["dave", "3", "20", "4", "1"]),
                line(["eve", "5", "50", "NULL", "NULL"]),
            ],
            "неделя 41: alice и bob делят первое место, carol и dave третье (четвёртого нет), eve новичок без прошлого места; на прошлой неделе bob и carol делили первое, alice была третьей, dave четвёртым; frank на этой неделе не играл и в списке быть не должен"
        );
    }

    #[tokio::test]
    async fn first_week_has_no_history() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            board(&db, 40).await,
            [
                line(["bob", "1", "0", "NULL", "NULL"]),
                line(["carol", "1", "0", "NULL", "NULL"]),
                line(["alice", "3", "20", "NULL", "NULL"]),
                line(["dave", "4", "30", "NULL", "NULL"]),
                line(["frank", "5", "50", "NULL", "NULL"]),
            ],
            "неделя 40: прошлой недели нет, поэтому prev_place и moved везде NULL"
        );
    }

    #[tokio::test]
    async fn previous_places_come_from_the_data() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO weekly_scores VALUES (5, 40, 70)").await;
        assert_eq!(
            board(&db, 41).await,
            [
                line(["alice", "1", "0", "4", "3"]),
                line(["bob", "1", "0", "1", "0"]),
                line(["carol", "3", "20", "1", "-2"]),
                line(["dave", "3", "20", "5", "2"]),
                line(["eve", "5", "50", "1", "-4"]),
            ],
            "после того как eve получила 70 очков на прошлой неделе, на первом месте их стало трое, а alice сползла на четвёртое: прошлые места считаются из таблицы, а не заранее"
        );
    }
}
