use std::path::PathBuf;

use grpc_kit::Stamp;
use post_service::{Config, run};
use proto::domain_code;
use proto::social::v1::posts_client::PostsClient;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{
    CommentRequest, CreatePostRequest, CreateProfileRequest, FeedRequest, GetPostRequest,
    PairRequest, Post, PostAction, TrendsRequest,
};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tonic::Code;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("порт 0 свободен всегда");
    let addr = listener.local_addr().expect("у слушателя есть адрес");
    (listener, addr.to_string())
}

fn lazy(addr: &str) -> Channel {
    Channel::from_shared(format!("http://{addr}"))
        .expect("адрес сервиса")
        .connect_lazy()
}

struct Pair {
    users: Channel,
    posts: Channel,
    stop: CancellationToken,
    tasks: Vec<JoinHandle<Result<(), String>>>,
}

impl Pair {
    async fn start(journal: Option<PathBuf>) -> Pair {
        let stop = CancellationToken::new();
        let (users_listener, users_addr) = listener().await;
        let users_task = tokio::spawn({
            let stop = stop.clone();
            async move {
                user_service::run(users_listener, user_service::Config::default(), stop)
                    .await
                    .map_err(|e| format!("{e:?}"))
            }
        });
        let mut pair = Pair::posts_for(users_addr, journal, stop).await;
        pair.tasks.push(users_task);
        pair
    }

    async fn posts_for(
        users_addr: String,
        journal: Option<PathBuf>,
        stop: CancellationToken,
    ) -> Pair {
        let (posts_listener, posts_addr) = listener().await;
        let config = Config {
            users: users_addr.clone(),
            journal,
            ..Config::default()
        };
        let posts_task = tokio::spawn({
            let stop = stop.clone();
            async move {
                run(posts_listener, config, stop)
                    .await
                    .map_err(|e| format!("{e:?}"))
            }
        });
        Pair {
            users: lazy(&users_addr),
            posts: lazy(&posts_addr),
            stop,
            tasks: vec![posts_task],
        }
    }

    fn posts(&self, user: u64) -> PostsClient<InterceptedService<Channel, Stamp>> {
        let stamp = Stamp::fresh().user(user).role("user");
        PostsClient::with_interceptor(self.posts.clone(), stamp)
    }

    fn anonymous(&self) -> PostsClient<Channel> {
        PostsClient::new(self.posts.clone())
    }

    fn users(&self, user: u64) -> UsersClient<InterceptedService<Channel, Stamp>> {
        UsersClient::with_interceptor(self.users.clone(), Stamp::fresh().user(user))
    }

    async fn profile(&mut self, name: &str) -> u64 {
        let ask = CreateProfileRequest {
            username: name.to_string(),
        };
        UsersClient::new(self.users.clone())
            .create_profile(ask)
            .await
            .unwrap_or_else(|e| panic!("профиль {name}: {e:?}"))
            .into_inner()
            .id
    }

    async fn post(&mut self, author: u64, text: &str) -> Post {
        let ask = CreatePostRequest {
            text: text.to_string(),
            ..Default::default()
        };
        self.posts(author)
            .create_post(ask)
            .await
            .unwrap_or_else(|e| panic!("пост «{text}»: {e:?}"))
            .into_inner()
    }

    async fn feed(&mut self, viewer: u64) -> Vec<String> {
        let ask = FeedRequest {
            page: 1,
            size: 20,
            ..FeedRequest::default()
        };
        let feed = self
            .posts(viewer)
            .feed(ask)
            .await
            .expect("лента")
            .into_inner();
        feed.posts.into_iter().map(|post| post.text).collect()
    }

    async fn stop(self) {
        self.stop.cancel();
        for task in self.tasks {
            let finished = task.await.expect("задача сервиса не падает");
            assert!(
                finished.is_ok(),
                "сервис останавливается без ошибки: {finished:?}"
            );
        }
    }
}

#[tokio::test]
async fn authors_are_checked_in_user_service() {
    let mut pair = Pair::start(None).await;
    let unknown = pair
        .posts(1)
        .create_post(CreatePostRequest {
            text: "кто я".to_string(),
            ..Default::default()
        })
        .await
        .expect_err("автора нет в user-service");
    assert_eq!(
        (unknown.code(), domain_code(&unknown)),
        (Code::InvalidArgument, Some("validation")),
        "автора не нашёл сосед, а для клиента это ошибка в запросе: {unknown:?}"
    );
    pair.profile("alice").await;
    let bob = pair.profile("bob").await;
    let post = pair.post(bob, "привет от bob").await;
    assert_eq!(
        (post.author_id, post.author_name.as_str()),
        (bob, "bob"),
        "post-service подтянул профили по порядку, и id совпали с user-service"
    );
    let empty = pair
        .posts(bob)
        .create_post(CreatePostRequest {
            text: "   ".to_string(),
            ..Default::default()
        })
        .await
        .expect_err("пустой пост");
    assert_eq!(domain_code(&empty), Some("validation"), "{empty:?}");
    let nobody = pair
        .anonymous()
        .create_post(CreatePostRequest {
            text: "чей я".to_string(),
            ..Default::default()
        })
        .await
        .expect_err("пост без x-user-id");
    assert_eq!(
        nobody.code(),
        Code::Unauthenticated,
        "автор приходит в метаданных x-user-id, без них непонятно, чей пост"
    );
    let public = pair
        .anonymous()
        .get_post(GetPostRequest { post_id: post.id })
        .await
        .expect("пост по ссылке открыт всем")
        .into_inner();
    assert_eq!(public.text, "привет от bob");
    pair.stop().await;
}

#[tokio::test]
async fn feed_follows_the_graph_kept_in_user_service() {
    let mut pair = Pair::start(None).await;
    let alice = pair.profile("alice").await;
    let bob = pair.profile("bob").await;
    let carol = pair.profile("carol").await;
    pair.post(bob, "пост bob").await;
    pair.post(carol, "пост carol").await;
    pair.post(alice, "пост alice").await;
    assert_eq!(
        pair.feed(alice).await,
        ["пост alice"],
        "пока ни на кого не подписана"
    );
    for target_id in [bob, carol] {
        let follow = PairRequest { target_id };
        pair.users(alice)
            .follow(follow)
            .await
            .expect("подписка в user-service");
    }
    assert_eq!(
        pair.feed(alice).await,
        ["пост alice", "пост carol", "пост bob"],
        "подписки живут в user-service, а лента их видит сразу"
    );
    pair.users(alice)
        .mute(PairRequest { target_id: bob })
        .await
        .expect("скрытие в user-service");
    assert_eq!(
        pair.feed(alice).await,
        ["пост alice", "пост carol"],
        "скрытый автор пропал из ленты"
    );
    let ghost = pair
        .posts(77)
        .feed(FeedRequest {
            page: 1,
            size: 20,
            ..FeedRequest::default()
        })
        .await
        .expect_err("лента несуществующего");
    assert_eq!(ghost.code(), Code::NotFound);
    pair.stop().await;
}

#[tokio::test]
async fn likes_comments_and_trends_live_in_post_service() {
    let mut pair = Pair::start(None).await;
    let alice = pair.profile("alice").await;
    let bob = pair.profile("bob").await;
    let quiet = pair.post(alice, "тихий пост").await;
    let loud = pair.post(alice, "громкий пост").await;
    let like = |post_id| PostAction { post_id };
    let first = pair
        .posts(bob)
        .like(like(loud.id))
        .await
        .expect("лайк")
        .into_inner();
    assert_eq!(
        (first.changed, first.author_id),
        (true, alice),
        "ответ на лайк называет автора: шлюзу нужно, кому слать пуш"
    );
    let again = pair
        .posts(bob)
        .like(like(loud.id))
        .await
        .expect("повтор")
        .into_inner();
    assert!(!again.changed);
    pair.posts(alice)
        .like(like(loud.id))
        .await
        .expect("свой лайк");
    pair.posts(bob).like(like(quiet.id)).await.expect("лайк");
    let trends = pair
        .anonymous()
        .trends(TrendsRequest { limit: 5 })
        .await
        .expect("тренды")
        .into_inner();
    let top: Vec<(u64, u64)> = trends.trends.iter().map(|t| (t.post_id, t.likes)).collect();
    assert_eq!(top, [(loud.id, 2), (quiet.id, 1)], "тренды по лайкам");
    let comment = pair
        .posts(bob)
        .comment(CommentRequest {
            parent_id: 0,
            post_id: loud.id,
            text: "согласен".to_string(),
        })
        .await
        .expect("комментарий")
        .into_inner();
    assert!(comment.comment_id > 0);
    let foreign = pair
        .posts(bob)
        .delete_post(like(loud.id))
        .await
        .expect_err("чужой пост не удалить");
    assert!(
        domain_code(&foreign).is_some(),
        "доменная ошибка с кодом: {foreign:?}"
    );
    let deleted = pair
        .posts(alice)
        .delete_post(like(loud.id))
        .await
        .expect("автор удаляет")
        .into_inner();
    assert!(deleted.changed);
    pair.stop().await;
}

#[tokio::test]
async fn fallen_user_service_is_unavailable() {
    let (dead, addr) = listener().await;
    drop(dead);
    let pair = Pair::posts_for(addr, None, CancellationToken::new()).await;
    let down = pair
        .posts(1)
        .create_post(CreatePostRequest {
            text: "есть кто".to_string(),
            ..Default::default()
        })
        .await
        .expect_err("user-service лежит");
    assert_eq!(
        (down.code(), domain_code(&down)),
        (Code::Unavailable, Some("unavailable")),
        "{down:?}"
    );
    assert!(
        down.message().contains("пользователей"),
        "понятно, кто именно недоступен: {down:?}"
    );
    pair.stop().await;
}

#[tokio::test]
async fn journal_keeps_posts_and_mirrored_profiles() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m30-posts");
    let _ = std::fs::remove_dir_all(&dir);
    let journal = dir.join("posts.ndjson");
    let mut pair = Pair::start(Some(journal.clone())).await;
    let alice = pair.profile("alice").await;
    pair.post(alice, "до перезапуска").await;
    pair.stop().await;

    let mut pair = Pair::start(Some(journal)).await;
    let again = pair.profile("alice").await;
    assert_eq!(
        again, alice,
        "свежий user-service выдаёт те же id по порядку"
    );
    assert_eq!(
        pair.feed(alice).await,
        ["до перезапуска"],
        "post-service поднимает из своего журнала и посты, и зеркало профилей"
    );
    let bob = pair.profile("bob").await;
    let post = pair.post(bob, "после перезапуска").await;
    assert_eq!(
        post.author_name, "bob",
        "зеркало продолжает с того же места"
    );
    pair.stop().await;
}
