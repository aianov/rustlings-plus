use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use contracts::{Origin, UserId};
use post_service::common::{Author, Graph};
use post_service::repository::BoxFuture;
use post_service::service::DirectoryError;
use post_service::{Draft, MemoryPosts, PostError, PostService, PostText, UserDirectory};

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

fn draft(text: &str) -> Draft {
    Draft {
        text: PostText::try_from(text).expect("текст поста"),
        tags: Vec::new(),
        links: Vec::new(),
    }
}

fn service() -> Arc<PostService> {
    Arc::new(PostService::new(Arc::new(MemoryPosts::new())))
}

#[tokio::test]
async fn cheap_checks_come_before_the_neighbour() {
    let posts = service();
    let users = Directory::default();
    for (text, code) in [
        ("", "blank"),
        ("   ", "blank"),
        (&"я".repeat(281), "too_long"),
    ] {
        let refused = PostText::try_from(text).expect_err("плохой текст");
        assert_eq!(refused.code, code, "{refused}");
    }
    assert_eq!(
        users.asked.load(Ordering::SeqCst),
        0,
        "пустой и длинный пост не собираются в PostText: до service и соседа они не доходят"
    );
    let post = posts
        .create(&users, UserId(2), draft("  привет  "), &Origin::new("t"))
        .await
        .expect("пост");
    assert_eq!(
        (post.author_name.as_str(), post.text.as_str()),
        ("user2", "привет"),
        "имя автора пришло через порт UserDirectory, текст обрезан"
    );
    let stranger = posts
        .create(&users, UserId(9), draft("кто я"), &Origin::new("t"))
        .await
        .expect_err("нет автора");
    assert!(
        matches!(stranger, PostError::UnknownAuthor(UserId(9))),
        "{stranger}"
    );
    assert_eq!(
        stranger.code(),
        "validation",
        "автора не нашёл сосед, но для клиента это ошибка в запросе, а не 404"
    );
    let down = Directory {
        down: true,
        ..Directory::default()
    };
    let unavailable = posts
        .create(&down, UserId(1), draft("есть кто"), &Origin::new("t"))
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
        .create(&users, UserId(1), draft("лайкни"), &Origin::new("t"))
        .await
        .expect("пост")
        .id;
    let mut tasks = Vec::new();
    for _ in 0..32 {
        let posts = Arc::clone(&posts);
        tasks.push(tokio::spawn(async move {
            posts.like(UserId(2), post, &Origin::new("t")).await
        }));
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
        .create(&users, UserId(1), draft("моё"), &Origin::new("t"))
        .await
        .expect("пост")
        .id;
    let followed = posts
        .create(&users, UserId(2), draft("подписка"), &Origin::new("t"))
        .await
        .expect("пост")
        .id;
    posts
        .create(&users, UserId(3), draft("скрытый"), &Origin::new("t"))
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
