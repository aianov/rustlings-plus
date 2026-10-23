use std::path::PathBuf;

use grpc_kit::{Stamp, channel};
use post_service::dto::draft;
use post_service::{Config, run};
use proto::from_status;
use proto::social::v1::posts_client::PostsClient;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{CreatePostRequest, CreateProfileRequest, GetPostRequest, Link};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tonic::Code;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;

fn link(url: &str, title: &str) -> Link {
    Link {
        url: url.to_string(),
        title: title.to_string(),
    }
}

fn ask(text: &str, tags: &[&str], links: Vec<Link>) -> CreatePostRequest {
    CreatePostRequest {
        text: text.to_string(),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        links,
    }
}

#[test]
fn a_draft_is_normalized_and_checked_as_a_whole() {
    let fine = draft(&ask(
        "  привет  ",
        &["#Rust", "tokio_2024"],
        vec![
            link(" https://docs.rs/tokio ", ""),
            link("https://tokio.rs", "  Сайт tokio  "),
        ],
    ))
    .expect("черновик верный");
    assert_eq!(fine.text.as_ref(), "привет");
    let tags: Vec<&str> = fine.tags.iter().map(AsRef::as_ref).collect();
    assert_eq!(
        tags,
        ["rust", "tokio_2024"],
        "решётка снята, регистр приведён"
    );
    assert_eq!(fine.links[0].url, "https://docs.rs/tokio");
    assert_eq!(
        fine.links[0].title, None,
        "пустая подпись это её отсутствие"
    );
    assert_eq!(fine.links[1].title.as_deref(), Some("Сайт tokio"));
    let broken = draft(&ask(
        "   ",
        &["rust", "c++", &"x".repeat(31)],
        vec![
            link("https://ok.example", &"т".repeat(81)),
            link("javascript:alert(1)", ""),
        ],
    ))
    .expect_err("ошибки");
    assert_eq!(
        broken.pairs(),
        [
            ("text", "blank"),
            ("tags[1]", "invalid_format"),
            ("tags[2]", "too_long"),
            ("links[0].title", "too_long"),
            ("links[1].url", "invalid_url"),
        ],
        "все ошибки разом, у каждой путь до поля"
    );
    let twins = draft(&ask("пост", &["Rust", "#rust"], Vec::new())).expect_err("повтор");
    assert_eq!(
        twins.pairs(),
        [("tags", "duplicate")],
        "повтор виден после нормализации"
    );
    let crowd =
        draft(&ask("пост", &["a", "b", "c", "d", "e", "f"], Vec::new())).expect_err("много");
    assert_eq!(crowd.pairs(), [("tags", "too_many")]);
    let links = (0..4)
        .map(|n| link(&format!("https://e{n}.example"), ""))
        .collect();
    let flood = draft(&ask("пост", &[], links)).expect_err("много ссылок");
    assert_eq!(flood.pairs(), [("links", "too_many")]);
}

struct Stack {
    users: Channel,
    posts: Channel,
    stop: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес").to_string();
    (listener, addr)
}

impl Stack {
    async fn start(journal: Option<PathBuf>, users_alive: bool) -> Stack {
        let stop = CancellationToken::new();
        let (users_listener, users_addr) = listener().await;
        let (posts_listener, posts_addr) = listener().await;
        let mut tasks = Vec::new();
        if users_alive {
            let config = user_service::Config::default();
            let users_stop = stop.clone();
            tasks.push(tokio::spawn(async move {
                let _ = user_service::run(users_listener, config, users_stop).await;
            }));
        } else {
            drop(users_listener);
        }
        let config = Config {
            users: users_addr.clone(),
            journal,
            ..Config::default()
        };
        let posts_stop = stop.clone();
        tasks.push(tokio::spawn(async move {
            let _ = run(posts_listener, config, posts_stop).await;
        }));
        Stack {
            users: channel(&users_addr, "user-service", None).expect("канал"),
            posts: channel(&posts_addr, "post-service", None).expect("канал"),
            stop,
            tasks,
        }
    }

    fn posts(&self, user: u64) -> PostsClient<InterceptedService<Channel, Stamp>> {
        PostsClient::with_interceptor(self.posts.clone(), Stamp::fresh().user(user))
    }

    async fn profile(&self, name: &str) -> u64 {
        let mut users = UsersClient::new(self.users.clone());
        let ask = CreateProfileRequest {
            username: name.to_string(),
        };
        users
            .create_profile(ask)
            .await
            .expect("профиль")
            .into_inner()
            .id
    }

    async fn stop(self) {
        self.stop.cancel();
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

#[tokio::test]
async fn tags_and_links_are_stored_and_survive_a_restart() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m35-posts");
    let _ = std::fs::remove_dir_all(&dir);
    let journal = dir.join("posts.ndjson");
    let stack = Stack::start(Some(journal.clone()), true).await;
    let alice = stack.profile("alice").await;
    let created = stack
        .posts(alice)
        .create_post(ask(
            "читаю про tokio",
            &["#Rust", "async"],
            vec![link("https://tokio.rs", "Сайт")],
        ))
        .await
        .expect("пост")
        .into_inner();
    assert_eq!(created.tags, ["rust", "async"]);
    assert_eq!(created.links, [link("https://tokio.rs", "Сайт")]);
    let refused = stack
        .posts(alice)
        .create_post(ask("", &["c++"], vec![link("ftp://x", "")]))
        .await
        .expect_err("кривой черновик");
    assert_eq!(refused.code(), Code::InvalidArgument);
    assert_eq!(
        from_status(&refused, "posts").fields(),
        [
            ("text", "blank"),
            ("tags[0]", "invalid_format"),
            ("links[0].url", "invalid_url")
        ]
    );
    stack.stop().await;
    let again = Stack::start(Some(journal), true).await;
    let restored = again
        .posts(alice)
        .get_post(GetPostRequest {
            post_id: created.id,
        })
        .await
        .expect("пост после перезапуска")
        .into_inner();
    assert_eq!(
        (restored.tags, restored.links),
        (created.tags, created.links),
        "теги и ссылки лежат в журнале вместе с постом"
    );
    again.stop().await;
}

#[tokio::test]
async fn the_neighbours_answer_is_translated_by_meaning() {
    let stack = Stack::start(None, true).await;
    let ghost = stack
        .posts(42)
        .create_post(ask("кто я", &[], Vec::new()))
        .await
        .expect_err("автора нет");
    assert_eq!(
        ghost.code(),
        Code::InvalidArgument,
        "user-service сказал NotFound про автора, а клиенту это ошибка в его запросе, не 404"
    );
    let body = from_status(&ghost, "posts");
    assert_eq!(body.fields(), [("author", "unknown")]);
    stack.stop().await;
    let lonely = Stack::start(None, false).await;
    let down = lonely
        .posts(1)
        .create_post(ask("есть кто", &[], Vec::new()))
        .await
        .expect_err("user-service лежит");
    assert_eq!(
        (down.code(), from_status(&down, "posts").code.as_str()),
        (Code::Unavailable, "unavailable"),
        "сосед лежит: временная ошибка, повторять можно"
    );
    lonely.stop().await;
}
