// Лента соцсети в Scylla: при публикации пост записывается в ленту каждого подписчика (fan-out на запись),
// и чтение ленты это одна партиция пользователя, свежие первыми. Свои посты автор тоже видит в ленте.
// Таблица feed_by_user (user_id, at, post_id, author_id), ключ (user_id, at, post_id), свежие первыми.
// TODO: `publish` возвращает скрипт, который раскладывает пост по лентам подписчиков и самого автора.

fn publish(author: i32, followers: &[i32], post_id: i32, at: i64) -> String {
    let _ = followers;
    format!("INSERT INTO feed_by_user (user_id, at, post_id, author_id) VALUES ({author}, {at}, {post_id}, {author})")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE feed_by_user (user_id int, at bigint, post_id int, author_id int, PRIMARY KEY (user_id, at, post_id)) WITH CLUSTERING ORDER BY (at DESC, post_id ASC)";

    #[tokio::test]
    async fn posts_fan_out_to_followers() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(&publish(1, &[7, 9], 100, 1000)).await;
        ks.run(&publish(2, &[7], 101, 1100)).await;
        ks.run(&publish(9, &[], 102, 1200)).await;
        ks.run(&publish(3, &[3, 9], 103, 1300)).await;
        let seven = ks.table("SELECT post_id, author_id FROM feed_by_user WHERE user_id = 7").await;
        assert_eq!(seven.rows, [["101", "2"], ["100", "1"]], "лента 7: посты тех, на кого он подписан, свежие первыми\n{seven}");
        let nine = ks.table("SELECT post_id FROM feed_by_user WHERE user_id = 9").await;
        assert_eq!(nine.column(0), ["103", "102", "100"], "в ленте 9 и чужие посты, и свой\n{nine}");
        let three = ks.table("SELECT post_id FROM feed_by_user WHERE user_id = 3").await;
        assert_eq!(three.column(0), ["103"], "автор, оказавшийся в своих подписчиках, получает пост один раз\n{three}");
    }
}
