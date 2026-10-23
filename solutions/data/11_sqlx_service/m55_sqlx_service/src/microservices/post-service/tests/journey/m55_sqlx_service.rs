use contracts::{Cause, Origin, UserId};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use post_service::common::now_micros;
use post_service::repository::{PostRepository, Search, SearchQueries, Sort, tags_with};
use post_service::{Draft, MIGRATIONS, PgPosts, PostText, Tag};
use tokio::sync::mpsc;

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_search_{name}_{}", std::process::id())).pool(4)
}

fn cause() -> Cause {
    Cause::new(&Origin::new("m55"), now_micros())
}

fn draft(text: &str, tags: &[&str]) -> Draft {
    Draft {
        text: PostText::try_from(text).expect("текст"),
        tags: tags
            .iter()
            .map(|tag| Tag::try_from(*tag).expect("тег"))
            .collect(),
        links: Vec::new(),
    }
}

fn texts(posts: &[post_service::common::Post]) -> Vec<String> {
    posts.iter().map(|post| post.text.clone()).collect()
}

#[tokio::test]
async fn search_filters_sorts_and_export_streams() {
    let config = database("find");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    let posts = PgPosts::open(pool.clone(), 7).await.expect("репозиторий");
    let (alice, bob) = (UserId(1), UserId(2));
    let full = posts
        .insert(
            alice,
            String::from("alice"),
            draft("скидка 100% на rust", &["rust", "sale"]),
            &cause(),
        )
        .await
        .expect("пост");
    let plain = posts
        .insert(
            alice,
            String::from("alice"),
            draft("скидка 100 на go", &["go"]),
            &cause(),
        )
        .await
        .expect("пост");
    let under = posts
        .insert(
            bob,
            String::from("bob"),
            draft("имя a_b в коде", &["rust"]),
            &cause(),
        )
        .await
        .expect("пост");
    let bare = posts
        .insert(
            bob,
            String::from("bob"),
            draft("имя axb в коде", &[]),
            &cause(),
        )
        .await
        .expect("пост");
    posts
        .like_if_absent(bob, plain.id, &cause())
        .await
        .expect("лайк")
        .expect("пост есть");
    let search =
        |text: Option<&str>, tag: Option<&str>, author: Option<UserId>, sort: Sort| Search {
            author,
            tag: tag.map(str::to_string),
            text: text.map(str::to_string),
            since: None,
            sort,
            limit: 20,
        };
    let percent = posts
        .search(search(Some("100%"), None, None, Sort::New))
        .await
        .expect("поиск");
    assert_eq!(
        texts(&percent),
        ["скидка 100% на rust"],
        "% в тексте это буква"
    );
    let underscore = posts
        .search(search(Some("a_b"), None, None, Sort::New))
        .await
        .expect("поиск");
    assert_eq!(
        texts(&underscore),
        ["имя a_b в коде"],
        "_ в тексте это буква"
    );
    let tagged = posts
        .search(search(None, Some("rust"), None, Sort::Old))
        .await
        .expect("поиск");
    assert_eq!(
        tagged.iter().map(|post| post.id).collect::<Vec<_>>(),
        [full.id, under.id]
    );
    assert_eq!(
        tagged[0].tags,
        ["rust", "sale"],
        "теги приходят пачкой через ANY"
    );
    let liked = posts
        .search(search(None, None, Some(alice), Sort::Likes))
        .await
        .expect("поиск");
    assert_eq!(liked[0].id, plain.id, "сортировка по белому списку");
    let mut tx = pool.begin().await.expect("транзакция");
    tags_with(
        &mut *tx,
        i64::try_from(bare.id.0).expect("id"),
        &[String::from("extra")],
    )
    .await
    .expect("та же функция в транзакции");
    tx.rollback().await.expect("откат");
    let rolled = posts
        .search(search(None, Some("extra"), None, Sort::New))
        .await
        .expect("поиск");
    assert!(rolled.is_empty(), "откат убрал теги");
    tags_with(
        &pool,
        i64::try_from(bare.id.0).expect("id"),
        &[String::from("extra")],
    )
    .await
    .expect("та же функция на пуле");
    let kept = posts
        .search(search(None, Some("extra"), None, Sort::New))
        .await
        .expect("поиск");
    assert_eq!(texts(&kept), ["имя axb в коде"]);
    let (sink, mut rows) = mpsc::channel(4);
    let sent = posts.export(alice, sink).await.expect("выгрузка");
    let mut exported = Vec::new();
    while let Some(post) = rows.recv().await {
        exported.push(post.text);
    }
    assert_eq!(sent, 2);
    assert_eq!(
        exported,
        ["скидка 100% на rust", "скидка 100 на go"],
        "поток, старые первыми"
    );
    let _ = drop_schema(&config).await;
}
