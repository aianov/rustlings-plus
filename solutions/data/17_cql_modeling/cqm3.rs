const BUCKETS: i32 = 8;

fn schema() -> &'static str {
    "CREATE TABLE activity (day int, bucket int, at bigint, user_id int, kind text, PRIMARY KEY ((day, bucket), at, user_id))"
}

fn bucket(user_id: i32) -> i32 {
    user_id.rem_euclid(BUCKETS)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    fn rows() -> String {
        (1..=400)
            .map(|u| format!("INSERT INTO activity (day, bucket, at, user_id, kind) VALUES (20261021, {}, {}, {u}, 'like')", bucket(u), 1000 + u))
            .collect::<Vec<_>>()
            .join(";")
    }

    #[test]
    fn buckets_are_in_range_and_even() {
        let mut seen = [0usize; 8];
        for u in 1..=800 {
            let b = bucket(u);
            assert!((0..8).contains(&b), "бакет пользователя {u} вне 0..7: {b}");
            seen[b as usize] += 1;
        }
        assert!(seen.iter().all(|&n| n == 100), "800 пользователей подряд должны разойтись по 100 на бакет: {seen:?}");
    }

    #[tokio::test]
    async fn a_day_is_spread_over_eight_partitions() {
        let ks = TestKs::with_cql(schema()).await;
        ks.run(&rows()).await;
        let parts = ks.table("SELECT DISTINCT day, bucket FROM activity").await;
        assert_eq!(parts.len(), 8, "день должен занимать восемь партиций\n{parts}");
        let all = ks.table("SELECT user_id FROM activity WHERE day = 20261021 AND bucket IN (0, 1, 2, 3, 4, 5, 6, 7)").await;
        assert_eq!(all.len(), 400, "чтение восьми бакетов отдаёт весь день\n{all}");
    }
}
