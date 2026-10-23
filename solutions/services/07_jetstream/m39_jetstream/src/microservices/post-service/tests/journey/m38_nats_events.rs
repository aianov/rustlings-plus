use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use contracts::{Event, EventBody, Origin, PostId, Topics, UserId};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL, Pattern};
use post_service::common::{Author, CommentText, Graph};
use post_service::repository::BoxFuture;
use post_service::service::DirectoryError;
use post_service::{
    BusEvents, Draft, Events, MemoryPosts, PostError, PostService, PostText, UserDirectory,
};

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

struct Nobody;

impl UserDirectory for Nobody {
    fn author(&self, _: UserId) -> BoxFuture<'_, Result<Option<Author>, DirectoryError>> {
        Box::pin(async { Ok(None) })
    }

    fn graph(&self, _: UserId) -> BoxFuture<'_, Result<Option<Graph>, DirectoryError>> {
        Box::pin(async { Ok(None) })
    }
}

#[derive(Default)]
struct Recorded(Mutex<Vec<(Event, Origin)>>);

impl Recorded {
    fn take(&self) -> Vec<(Event, Origin)> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Events for Recorded {
    fn publish(&self, event: Event, origin: Origin) -> BoxFuture<'_, ()> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((event, origin));
        Box::pin(async {})
    }
}

fn draft(raw: &str) -> Draft {
    Draft {
        text: PostText::try_from(raw).expect("текст"),
        tags: Vec::new(),
        links: Vec::new(),
    }
}

fn comment(raw: &str) -> CommentText {
    CommentText::try_from(raw).expect("комментарий")
}

#[tokio::test]
async fn every_change_of_a_post_becomes_a_fact_after_it_happened() {
    let recorded = Arc::new(Recorded::default());
    let posts = PostService::new(Arc::new(MemoryPosts::new()))
        .with_events(Arc::clone(&recorded) as Arc<dyn Events>);
    let origin = Origin::new("e856e880e9a85e6d");
    let (alice, bob, carol) = (UserId(1), UserId(2), UserId(3));
    let post = posts
        .create(&Everyone, alice, draft("привет"), &origin)
        .await
        .expect("пост");
    let liked = posts.like(bob, post.id, &origin).await.expect("лайк");
    assert!(liked.changed);
    let again = posts.like(bob, post.id, &origin).await.expect("повтор");
    assert!(!again.changed);
    let own = posts
        .like(alice, post.id, &origin)
        .await
        .expect("свой лайк");
    assert!(own.changed);
    posts
        .comment(carol, post.id, comment("согласна"), &origin)
        .await
        .expect("комментарий");
    let facts: Vec<Event> = recorded
        .take()
        .into_iter()
        .map(|(event, seen)| {
            assert_eq!(
                seen, origin,
                "у каждого события номер запроса, который его вызвал"
            );
            event
        })
        .collect();
    assert_eq!(
        facts,
        [
            Event::post_created(alice, post.id),
            Event::post_liked(bob, alice, post.id),
            Event::post_liked(alice, alice, post.id),
            Event::post_commented(carol, alice, post.id),
        ],
        "событие это факт: создан, лайкнут (и своим лайком тоже), прокомментирован; \
         владелец события это автор поста, повторный лайк ничего не изменил и события не дал"
    );
}

#[tokio::test]
async fn a_refused_change_publishes_nothing() {
    let recorded = Arc::new(Recorded::default());
    let posts = PostService::new(Arc::new(MemoryPosts::new()))
        .with_events(Arc::clone(&recorded) as Arc<dyn Events>);
    let origin = Origin::new("e856e880e9a85e6d");
    assert!(matches!(
        posts
            .create(&Nobody, UserId(9), draft("кто я"), &origin)
            .await,
        Err(PostError::UnknownAuthor(_))
    ));
    assert!(matches!(
        posts.like(UserId(2), PostId(77), &origin).await,
        Err(PostError::UnknownPost(_))
    ));
    assert!(matches!(
        posts
            .comment(UserId(2), PostId(77), comment("эй"), &origin)
            .await,
        Err(PostError::UnknownPost(_))
    ));
    assert_eq!(
        recorded.take(),
        [],
        "отказ ничего не меняет и событий не даёт"
    );
}

#[tokio::test]
async fn the_bus_adapter_names_the_author_in_the_subject() {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    let topics = Topics::new(&format!(
        "p{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let url = std::env::var("RUSTLINGS_NATS_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string());
    let bus = Bus::connect(BusConfig::new(url.clone()))
        .await
        .expect("NATS курса запущен?");
    let watch = Pattern::parse(&topics.all_events().join(".")).expect("шаблон");
    let mut seen = bus.subscribe::<EventBody>(&watch).await.expect("подписка");
    let posts = PostService::new(Arc::new(MemoryPosts::new())).with_events(Arc::new(
        BusEvents::open(bus.clone(), topics.clone())
            .await
            .expect("stream событий готов"),
    ));
    let post = posts
        .create(&Everyone, UserId(1), draft("в шину"), &Origin::new("req-1"))
        .await
        .expect("пост");
    let got = seen
        .next_within(Duration::from_secs(3))
        .await
        .expect("событие пришло")
        .expect("событие разобралось");
    assert_eq!(
        (got.subject.as_str().to_string(), got.body),
        (
            format!("{}.events.post.created.u_1", topics.root()),
            EventBody {
                actor: UserId(1),
                post: Some(post.id)
            }
        )
    );
    assert_eq!(
        got.meta.map(|meta| meta.request_id),
        Some(String::from("req-1"))
    );
    if let Ok(client) = async_nats::connect(url).await {
        let _ = async_nats::jetstream::new(client)
            .delete_stream(topics.stream())
            .await;
    }
}
