use grpc_kit::Stamp;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{
    AvatarChunk, AvatarRequest, CreateProfileRequest, DeleteAvatarRequest, FindProfileRequest,
    GetProfileRequest, ListProfilesRequest, PairRequest, RelationsRequest,
};
use proto::{MESSAGE_MAX, domain_code};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tonic::Code;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;
use user_service::{Config, LIST_LIMIT, run};

struct Service {
    client: UsersClient<Channel>,
    channel: Channel,
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
        let channel = Channel::from_shared(format!("http://{addr}"))
            .expect("адрес user-service")
            .connect()
            .await
            .expect("user-service принимает соединения");
        let client = UsersClient::new(channel.clone())
            .max_decoding_message_size(MESSAGE_MAX)
            .max_encoding_message_size(MESSAGE_MAX);
        Service {
            client,
            channel,
            stop,
            task,
        }
    }

    fn by(&self, user: u64) -> UsersClient<InterceptedService<Channel, Stamp>> {
        UsersClient::with_interceptor(self.channel.clone(), Stamp::fresh().user(user))
            .max_decoding_message_size(MESSAGE_MAX)
            .max_encoding_message_size(MESSAGE_MAX)
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
        let ask = PairRequest { target_id };
        let mut client = self.by(user_id);
        let answer = if mute {
            client.mute(ask).await
        } else {
            client.follow(ask).await
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
        .by(1)
        .follow(PairRequest { target_id: 1 })
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

fn png(width: u32, height: u32, size: usize) -> Vec<u8> {
    let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    data.resize(size.max(data.len()), 0);
    data
}

fn chunks(data: &[u8]) -> impl futures::Stream<Item = AvatarChunk> + Send + 'static {
    let pieces: Vec<AvatarChunk> = data
        .chunks(16 * 1024)
        .map(|piece| AvatarChunk {
            data: piece.to_vec(),
        })
        .collect();
    futures::stream::iter(pieces)
}

#[tokio::test]
async fn avatars_stream_in_chunks() {
    let mut users = Service::start().await;
    users.create("alice").await;
    let big = png(64, 64, 5 * 1024 * 1024);
    let created = users
        .by(1)
        .upload_avatar(chunks(&big))
        .await
        .expect("5 МиБ идут потоком кусков")
        .into_inner();
    assert!(created.created);
    let small = png(8, 8, 300);
    let replaced = users
        .by(1)
        .upload_avatar(chunks(&small))
        .await
        .expect("замена")
        .into_inner();
    assert!(!replaced.created, "второй аватар заменяет первый");
    let text = users
        .by(1)
        .upload_avatar(chunks(b"just some text, not a picture"))
        .await
        .expect_err("не картинка");
    assert_eq!(domain_code(&text), Some("unsupported"), "{text:?}");
    let stranger = users
        .by(9)
        .upload_avatar(chunks(&small))
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
        ("image/png", small.as_slice())
    );
    let anonymous = users
        .client
        .delete_avatar(DeleteAvatarRequest {})
        .await
        .expect_err("без x-user-id непонятно, чей аватар");
    assert_eq!(anonymous.code(), Code::Unauthenticated);
    let removed = users
        .by(1)
        .delete_avatar(DeleteAvatarRequest {})
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
