use contracts::{Cause, CommentId, Origin, UserId};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use post_service::common::now_micros;
use post_service::repository::{DraftRepository, PostRepository, ThreadQueries};
use post_service::{CommentText, Draft, MIGRATIONS, PgPosts, PostText};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_threads_{name}_{}", std::process::id())).pool(2)
}

fn cause() -> Cause {
    Cause::new(&Origin::new("m53"), now_micros())
}

fn draft(text: &str) -> Draft {
    Draft {
        text: PostText::try_from(text).expect("текст"),
        tags: Vec::new(),
        links: Vec::new(),
    }
}

fn said(text: &str) -> CommentText {
    CommentText::try_from(text).expect("комментарий")
}

#[tokio::test]
async fn threads_latest_comments_and_weekly_top() {
    let config = database("tree");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    let posts = PgPosts::open(pool, 5).await.expect("репозиторий");
    let (alice, bob, carol, dave) = (UserId(1), UserId(2), UserId(3), UserId(4));
    let first = posts
        .insert(alice, String::from("alice"), draft("первый"), &cause())
        .await
        .expect("пост");
    let second = posts
        .insert(alice, String::from("alice"), draft("второй"), &cause())
        .await
        .expect("пост");
    let third = posts
        .insert(dave, String::from("dave"), draft("от dave"), &cause())
        .await
        .expect("пост");
    let root = posts
        .comment(bob, first.id, said("корень"), &cause())
        .await
        .expect("комментарий")
        .expect("пост есть");
    let answer = posts
        .reply(carol, first.id, root, said("ответ"), &cause())
        .await
        .expect("ответ")
        .expect("родитель есть");
    let deeper = posts
        .reply(alice, first.id, answer, said("глубже"), &cause())
        .await
        .expect("ответ")
        .expect("родитель есть");
    let other = posts
        .comment(dave, first.id, said("второй корень"), &cause())
        .await
        .expect("комментарий")
        .expect("пост есть");
    assert_eq!(
        posts
            .reply(bob, second.id, root, said("чужая ветка"), &cause())
            .await
            .expect("запрос"),
        None,
        "родитель из другого поста не годится"
    );
    let tree = posts.thread(first.id, 32).await;
    let shape: Vec<(CommentId, u32, u64)> = tree
        .iter()
        .map(|node| (node.id, node.depth, node.replies))
        .collect();
    assert_eq!(
        shape,
        [(root, 1, 1), (answer, 2, 1), (deeper, 3, 0), (other, 1, 0)],
        "порядок по пути в дереве, глубина и число ответов"
    );
    assert_eq!(tree[1].parent, Some(root));
    let shallow: Vec<CommentId> = posts
        .thread(first.id, 2)
        .await
        .iter()
        .map(|node| node.id)
        .collect();
    assert_eq!(shallow, [root, answer, other], "предел глубины");
    assert!(
        posts
            .remove_comment(root, &cause())
            .await
            .expect("удаление"),
        "корень удалён"
    );
    let stubbed = posts.thread(first.id, 32).await;
    assert!(
        stubbed[0].deleted && stubbed[0].text.is_empty(),
        "заглушка удалённого с ответами"
    );
    assert_eq!(stubbed.len(), 4);
    let latest = posts.latest(vec![first.id, second.id]).await;
    let ids: Vec<(u64, Vec<CommentId>)> = latest
        .iter()
        .map(|(post, lines)| (post.0, lines.iter().map(|line| line.id).collect()))
        .collect();
    assert_eq!(
        ids,
        [
            (first.id.0, vec![other, deeper, answer]),
            (second.id.0, vec![])
        ],
        "три последних под каждым постом, пост без комментариев тоже есть"
    );
    for (liker, post) in [
        (bob, first.id),
        (carol, first.id),
        (bob, third.id),
        (carol, third.id),
    ] {
        posts
            .like_if_absent(liker, post, &cause())
            .await
            .expect("лайк")
            .expect("пост есть");
    }
    let top = posts.top(now_micros(), 10).await;
    let places: Vec<(UserId, u64, u64)> = top
        .iter()
        .map(|rank| (rank.author, rank.place, rank.likes))
        .collect();
    assert_eq!(
        places,
        [(alice, 1, 2), (dave, 1, 2)],
        "ничья делит место: RANK"
    );
    assert_eq!(top[0].shift, 0, "прошлой недели не было");
    let _ = drop_schema(&config).await;
}
