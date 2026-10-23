use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{
    AvatarRequest, CreateProfileRequest, FindProfileRequest, GetProfileRequest,
    ListProfilesRequest, PairRequest, PutAvatarRequest, RelationsRequest,
};
use proto::{MESSAGE_MAX, domain_code};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tonic::Code;
use tonic::transport::Channel;
use user_service::{Config, LIST_LIMIT, run};

struct Service {
    client: UsersClient<Channel>,
    stop: CancellationToken,
    task: JoinHandle<user_service::Result<()>>,
}

impl Service {
    async fn start() -> Service {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("порт 0 свободен всегда");
        let addr = listener.local_addr().expect("у слушателя есть адрес");
        let stop = CancellationToken::new();
        let task = tokio::spawn(run(listener, Config::default(), stop.clone()));
        let client = UsersClient::connect(format!("http://{addr}"))
            .await
            .expect("user-service принимает соединения")
            .max_decoding_message_size(MESSAGE_MAX)
            .max_encoding_message_size(MESSAGE_MAX);
        Service { client, stop, task }
    }

    async fn create(&mut self, name: &str) -> u64 {
        let ask = CreateProfileRequest {
            username: name.to_string(),
        };
        self.client
            .create_profile(ask)
            .await
            .unwrap_or_else(|e| panic!("профиль {name} создаётся: {e:?}"))
            .into_inner()
            .id
    }

    async fn pair(&mut self, mute: bool, user_id: u64, target_id: u64) -> bool {
        let ask = PairRequest { user_id, target_id };
        let answer = if mute {
            self.client.mute(ask).await
        } else {
            self.client.follow(ask).await
        };
        answer.expect("связь ставится").into_inner().changed
    }

    async fn stop(self) {
        self.stop.cancel();
        let finished = self.task.await.expect("задача user-service не падает");
        assert!(
            finished.is_ok(),
            "user-service останавливается без ошибки: {finished:?}"
        );
    }
}

#[tokio::test]
async fn profiles_by_id_by_name_and_in_pages() {
    let mut users = Service::start().await;
    let mut ids = Vec::new();
    for name in ["alice", "bob", "carol", "dave", "erin"] {
        ids.push(users.create(name).await);
    }
    assert_eq!(ids, [1, 2, 3, 4, 5], "id выдаются по порядку с единицы");
    let bob = users
        .client
        .get_profile(GetProfileRequest { user_id: 2 })
        .await
        .expect("bob есть")
        .into_inner();
    assert_eq!(bob.username, "bob");
    let carol = users
        .client
        .find_profile(FindProfileRequest {
            username: "Carol".to_string(),
        })
        .await
        .expect("поиск без учёта регистра")
        .into_inner();
    assert_eq!(carol.id, 3);
    let page = users
        .client
        .list_profiles(ListProfilesRequest { after: 1, limit: 2 })
        .await
        .expect("страница профилей")
        .into_inner();
    let names: Vec<&str> = page.profiles.iter().map(|p| p.username.as_str()).collect();
    assert_eq!(names, ["bob", "carol"], "после id 1 не больше двух");
    let tail = users
        .client
        .list_profiles(ListProfilesRequest { after: 3, limit: 0 })
        .await
        .expect("хвост")
        .into_inner();
    assert_eq!(
        tail.profiles.iter().map(|p| p.id).collect::<Vec<_>>(),
        [4, 5],
        "limit 0 значит «сколько разрешено» ({LIST_LIMIT})"
    );
    let empty = users
        .client
        .list_profiles(ListProfilesRequest {
            after: u64::MAX,
            limit: 10,
        })
        .await
        .expect("после последнего пусто, без паники на переполнении")
        .into_inner();
    assert!(empty.profiles.is_empty());
    users.stop().await;
}

#[tokio::test]
async fn domain_errors_carry_their_code() {
    let mut users = Service::start().await;
    users.create("alice").await;
    let taken = users
        .client
        .create_profile(CreateProfileRequest {
            username: "ALICE".to_string(),
        })
        .await
        .expect_err("ник занят");
    assert_eq!(
        (taken.code(), domain_code(&taken)),
        (Code::AlreadyExists, Some("conflict")),
        "{taken:?}"
    );
    let missing = users
        .client
        .get_profile(GetProfileRequest { user_id: 42 })
        .await
        .expect_err("профиля 42 нет");
    assert_eq!(
        (missing.code(), domain_code(&missing)),
        (Code::NotFound, Some("not_found")),
        "{missing:?}"
    );
    let selfish = users
        .client
        .follow(PairRequest {
            user_id: 1,
            target_id: 1,
        })
        .await
        .expect_err("на себя не подписываются");
    assert_eq!(
        (selfish.code(), domain_code(&selfish)),
        (Code::FailedPrecondition, Some("rejected")),
        "{selfish:?}"
    );
    assert!(
        !selfish.message().is_empty(),
        "текст доменной ошибки читаем"
    );
    users.stop().await;
}

#[tokio::test]
async fn relations_know_who_sees_the_posts() {
    let mut users = Service::start().await;
    for name in ["alice", "bob", "carol", "dave"] {
        users.create(name).await;
    }
    assert!(users.pair(false, 2, 1).await, "bob подписан на alice");
    assert!(users.pair(false, 3, 1).await, "carol подписана на alice");
    assert!(!users.pair(false, 3, 1).await, "повтор ничего не меняет");
    assert!(users.pair(true, 3, 1).await, "carol скрыла alice");
    assert!(users.pair(false, 1, 4).await, "alice подписана на dave");
    assert!(users.pair(true, 1, 2).await, "alice скрыла bob");
    let alice = users
        .client
        .relations(RelationsRequest { user_id: 1 })
        .await
        .expect("связи alice")
        .into_inner();
    assert_eq!(alice.following, [4]);
    assert_eq!(alice.followers, [2, 3]);
    assert_eq!(alice.muted, [2]);
    assert_eq!(
        alice.audience,
        [2],
        "посты alice видит bob, а carol подписана, но скрыла её"
    );
    let ghost = users
        .client
        .relations(RelationsRequest { user_id: 9 })
        .await
        .expect_err("связей несуществующего нет");
    assert_eq!(ghost.code(), Code::NotFound);
    users.stop().await;
}

#[tokio::test]
async fn avatars_are_unary_and_big() {
    let mut users = Service::start().await;
    users.create("alice").await;
    let five = vec![7u8; 5 * 1024 * 1024];
    let put = |user_id, content_type: &str, data: Vec<u8>| PutAvatarRequest {
        user_id,
        content_type: content_type.to_string(),
        data,
    };
    let created = users
        .client
        .put_avatar(put(1, "image/png", five.clone()))
        .await
        .expect("5 МиБ проходят одним сообщением")
        .into_inner();
    assert!(created.created);
    let replaced = users
        .client
        .put_avatar(put(1, "image/webp", vec![1, 2, 3]))
        .await
        .expect("замена")
        .into_inner();
    assert!(!replaced.created, "второй аватар заменяет первый");
    let text = users
        .client
        .put_avatar(put(1, "text/plain", vec![1]))
        .await
        .expect_err("не картинка");
    assert_eq!(domain_code(&text), Some("unsupported"), "{text:?}");
    let stranger = users
        .client
        .put_avatar(put(9, "image/png", vec![1]))
        .await
        .expect_err("аватар несуществующего");
    assert_eq!(stranger.code(), Code::NotFound);
    let got = users
        .client
        .get_avatar(AvatarRequest { user_id: 1 })
        .await
        .expect("аватар читается")
        .into_inner();
    assert_eq!(
        (got.content_type.as_str(), got.data.as_slice()),
        ("image/webp", [1u8, 2, 3].as_slice())
    );
    let removed = users
        .client
        .delete_avatar(AvatarRequest { user_id: 1 })
        .await
        .expect("удаление")
        .into_inner();
    assert!(removed.changed);
    let gone = users
        .client
        .get_avatar(AvatarRequest { user_id: 1 })
        .await
        .expect_err("аватара больше нет");
    assert_eq!(gone.code(), Code::NotFound);
    users.stop().await;
}
