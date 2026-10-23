use std::sync::Arc;

use contracts::{Origin, UserId};
use post_service::common::{Author, Graph};
use post_service::repository::{BoxFuture, OutboxRepository, PostRepository};
use post_service::service::DirectoryError;
use post_service::{Draft, MemoryPosts, PostService, PostText, UserDirectory};
use service_runtime::split;

struct Everyone;

impl UserDirectory for Everyone {
    fn author(&self, id: UserId) -> BoxFuture<'_, Result<Option<Author>, DirectoryError>> {
        Box::pin(async move {
            Ok(Some(Author {
                id,
                name: format!("user{}", id.0),
            }))
        })
    }

    fn graph(&self, _: UserId) -> BoxFuture<'_, Result<Option<Graph>, DirectoryError>> {
        Box::pin(async { Ok(Some(Graph::default())) })
    }
}

fn draft(text: &str) -> Draft {
    Draft {
        text: PostText::try_from(text).expect("текст"),
        tags: Vec::new(),
        links: Vec::new(),
    }
}

#[tokio::test]
async fn post_ids_are_sortable_and_carry_the_node() {
    let store = Arc::new(MemoryPosts::new().on_node(5));
    let posts = PostService::new(store);
    let origin = Origin::new("m48");
    let mut last = 0;
    for n in 0..50 {
        let post = posts
            .create(&Everyone, UserId(1), draft(&format!("пост {n}")), &origin)
            .await
            .expect("пост");
        assert!(post.id.0 > last, "id растут вместе со временем");
        assert_eq!(split(post.id.0).node, 5, "в id номер узла");
        last = post.id.0;
    }
}

#[tokio::test]
async fn likes_pile_up_and_flush_as_one_batch() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m48-likes");
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("posts.ndjson");
    let (journal, entries) = journal::Journal::open(Some(&file)).await.expect("журнал");
    let store = Arc::new(MemoryPosts::restore(journal, entries));
    let posts = Arc::new(PostService::new(store.clone()));
    let origin = Origin::new("m48");
    let post = posts
        .create(&Everyone, UserId(1), draft("лайкайте"), &origin)
        .await
        .expect("пост")
        .id;
    let lines = || {
        std::fs::read_to_string(&file)
            .unwrap_or_default()
            .lines()
            .count()
    };
    let written = lines();
    let mut tasks = Vec::new();
    for fan in 2..=41u64 {
        let posts = Arc::clone(&posts);
        let origin = origin.clone();
        tasks.push(tokio::spawn(async move {
            posts.like(UserId(fan), post, &origin).await
        }));
    }
    for task in tasks {
        assert!(task.await.expect("задача").expect("лайк").changed);
    }
    assert_eq!(
        posts.public(post).await.expect("пост").likes,
        40,
        "счётчик в памяти сразу"
    );
    assert_eq!(lines(), written, "в журнал лайки ещё не ушли");
    assert_eq!(
        store.flush().await.expect("сброс"),
        40,
        "сорок лайков одной пачкой"
    );
    assert_eq!(lines(), written + 40);
    assert_eq!(store.flush().await.expect("повтор"), 0);
    let _ = posts.like(UserId(99), post, &origin).await.expect("лайк");
    let rows = store.unpublished(100).await;
    assert!(
        rows.iter().any(|row| row.event.kind == "post.liked"),
        "выборка релея сама сбрасывает накопленные лайки"
    );
    drop(posts);
    let (journal, entries) = journal::Journal::open(Some(&file)).await.expect("журнал");
    let again = MemoryPosts::restore(journal, entries);
    assert_eq!(
        again.get(post).await.expect("пост").likes,
        41,
        "после перезапуска все лайки на месте"
    );
}
