use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use contracts::UserId;
use post_service::common::{Author, Graph};
use post_service::repository::BoxFuture;
use post_service::service::DirectoryError;
use post_service::{MemoryPosts, PostError, PostService, UserDirectory};

#[derive(Default)]
struct Directory {
    asked: AtomicUsize,
    down: bool,
    graph: Graph,
}

impl UserDirectory for Directory {
    fn author(&self, id: UserId) -> BoxFuture<'_, Result<Option<Author>, DirectoryError>> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if self.down {
                return Err(DirectoryError::Unavailable);
            }
            Ok((id.0 <= 3).then(|| Author {
                id,
                name: format!("user{}", id.0),
            }))
        })
    }

    fn graph(&self, viewer: UserId) -> BoxFuture<'_, Result<Option<Graph>, DirectoryError>> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if self.down {
                return Err(DirectoryError::Unavailable);
            }
            Ok((viewer.0 <= 3).then(|| self.graph.clone()))
        })
    }
}

fn service() -> Arc<PostService> {
    Arc::new(PostService::new(Arc::new(MemoryPosts::new())))
}

#[tokio::test]
async fn cheap_checks_come_before_the_neighbour() {
    let posts = service();
    let users = Directory::default();
    for text in ["", "   ", &"я".repeat(281)] {
        let refused = posts
            .create(&users, UserId(1), text)
            .await
            .expect_err("плохой текст");
        assert_eq!(refused.code(), "rejected", "{refused}");
    }
    assert_eq!(
        users.asked.load(Ordering::SeqCst),
        0,
        "пустой и длинный пост отсекаются у себя, без похода в user-service"
    );
    let post = posts
        .create(&users, UserId(2), "  привет  ")
        .await
        .expect("пост");
    assert_eq!(
        (post.author_name.as_str(), post.text.as_str()),
        ("user2", "привет"),
        "имя автора пришло через порт UserDirectory, текст обрезан"
    );
    let stranger = posts
        .create(&users, UserId(9), "кто я")
        .await
        .expect_err("нет автора");
    assert!(
        matches!(stranger, PostError::UnknownAuthor(UserId(9))),
        "{stranger}"
    );
    assert_eq!(stranger.code(), "not_found");
    let down = Directory {
        down: true,
        ..Directory::default()
    };
    let unavailable = posts
        .create(&down, UserId(1), "есть кто")
        .await
        .expect_err("сосед лежит");
    assert_eq!(
        unavailable.code(),
        "unavailable",
        "«автора нет» и «не смогли спросить» это разные ответы: {unavailable}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn one_like_per_user_even_at_once() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m33-likes");
    let _ = std::fs::remove_dir_all(&dir);
    let (journal, entries) = journal::Journal::open(Some(&dir.join("posts.ndjson")))
        .await
        .expect("журнал на диске: запись в него уступает поток, и гонка становится видна");
    let posts = Arc::new(PostService::new(Arc::new(MemoryPosts::restore(
        journal, entries,
    ))));
    let users = Directory::default();
    let post = posts
        .create(&users, UserId(1), "лайкни")
        .await
        .expect("пост")
        .id;
    let mut tasks = Vec::new();
    for _ in 0..32 {
        let posts = Arc::clone(&posts);
        tasks.push(tokio::spawn(
            async move { posts.like(UserId(2), post).await },
        ));
    }
    let mut changed = 0;
    for task in tasks {
        if task.await.expect("задача").expect("лайк").changed {
            changed += 1;
        }
    }
    assert_eq!(
        changed, 1,
        "лайк ставится атомарно в репозитории: like_if_absent"
    );
    assert_eq!(posts.public(post).await.expect("пост").likes, 1);
}

#[tokio::test]
async fn feed_and_delete_follow_the_rules() {
    let posts = service();
    let users = Directory {
        graph: Graph {
            following: vec![UserId(2), UserId(3)],
            muted: vec![UserId(3)],
        },
        ..Directory::default()
    };
    let own = posts
        .create(&users, UserId(1), "моё")
        .await
        .expect("пост")
        .id;
    let followed = posts
        .create(&users, UserId(2), "подписка")
        .await
        .expect("пост")
        .id;
    posts
        .create(&users, UserId(3), "скрытый")
        .await
        .expect("пост");
    let feed: Vec<_> = posts
        .feed(&users, UserId(1), 1, 10)
        .await
        .expect("лента")
        .into_iter()
        .map(|post| post.id)
        .collect();
    assert_eq!(
        feed,
        [followed, own],
        "подписки и свои посты, скрытые авторы выпадают"
    );
    let foreign = posts.delete(UserId(2), own).await.expect_err("чужой пост");
    assert_eq!(foreign.code(), "forbidden", "{foreign}");
    assert!(posts.delete(UserId(1), own).await.expect("автор удаляет"));
    let gone = posts.public(own).await.expect_err("удалённого поста нет");
    assert_eq!(gone.code(), "not_found");
}
