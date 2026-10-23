// Две пары правил уникальности. Первое: у двух пользователей не может быть одной почты.
// Второе: дружба. Строка (1, 2) и строка (2, 1) это одна и та же дружба, а в таблице она
// должна быть записана ровно один раз, в каком бы порядке её ни вставляли. Дружить с собой
// нельзя. Ограничения нужно назвать самим, чтобы в логах ошибок было видно, какое нарушено:
// почта это `users_email_uniq`, повтор пары это `friendships_pair_uniq`.
// TODO: Добавь ограничения в таблицы.

fn uniqueness() -> &'static str {
    "CREATE TABLE users (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        email text NOT NULL
    );
    CREATE TABLE friendships (
        user_a bigint NOT NULL,
        user_b bigint NOT NULL
    )"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    #[tokio::test]
    async fn email_is_unique() {
        let db = TestDb::new().await;
        db.run(uniqueness()).await;
        db.run("INSERT INTO users (email) VALUES ('a@x.org')").await;
        let err = db.fails("INSERT INTO users (email) VALUES ('a@x.org')").await.expect("повторная почта должна отвергаться");
        assert_eq!(err.code, "23505", "нарушение уникальности: {}", err.message);
        assert_eq!(err.constraint.as_deref(), Some("users_email_uniq"), "ограничение должно называться users_email_uniq");
    }

    #[tokio::test]
    async fn same_pair_twice_is_rejected() {
        let db = TestDb::new().await;
        db.run(uniqueness()).await;
        db.run("INSERT INTO friendships VALUES (1, 2), (1, 3), (2, 3)").await;
        let err = db.fails("INSERT INTO friendships VALUES (1, 2)").await.expect("повтор той же пары должен отвергаться");
        assert_eq!(err.constraint.as_deref(), Some("friendships_pair_uniq"), "ограничение повтора должно называться friendships_pair_uniq. База: {}", err.message);
    }

    #[tokio::test]
    async fn reversed_pair_is_the_same_friendship() {
        let db = TestDb::new().await;
        db.run(uniqueness()).await;
        db.run("INSERT INTO friendships VALUES (1, 2)").await;
        let reversed = db.fails("INSERT INTO friendships VALUES (2, 1)").await;
        assert!(reversed.is_some(), "пара (2, 1) это та же дружба, что (1, 2): вторая запись должна отвергаться, а не появляться дублем");
        let n = db.table("SELECT count(*) FROM friendships").await.cell(0, 0).to_string();
        assert_eq!(n, "1", "в таблице должна остаться одна дружба");
    }

    #[tokio::test]
    async fn nobody_befriends_themselves() {
        let db = TestDb::new().await;
        db.run(uniqueness()).await;
        assert!(db.fails("INSERT INTO friendships VALUES (4, 4)").await.is_some(), "дружить с собой нельзя: пара (4, 4) должна отвергаться");
        db.run("INSERT INTO friendships VALUES (4, 5)").await;
    }
}
