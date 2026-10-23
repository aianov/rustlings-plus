use std::error::Error as _;
use std::fmt;
use std::io;
use std::sync::Arc;

use contracts::{
    Call, Changed, ChatHistory, Envelope, ErrorBody, FeedPage, MessageView, Outcome, PostId,
    PostView, Push, Reply, Response, UserId, UserView, Username, write_line,
};
use social_core::{
    App, DomainError, Error, MemNetwork, MemPostStore, MemUserStore, Notification, Notifiers,
    PushQueue, report, serve, serve_line,
};
use tokio::time::{Duration, timeout};

type Shared = App<MemUserStore, MemPostStore>;

fn shared() -> Shared {
    App::new(MemNetwork::new()).expect("ядро запускается внутри рантайма")
}

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник проходит проверку")
}

async fn ok(net: &Shared, call: Call) -> Reply {
    match serve(net, &call).await {
        Ok(reply) => reply,
        Err(error) => panic!("{call:?} должен пройти, а вернул {}", report(&error)),
    }
}

async fn fail(net: &Shared, call: Call) -> ErrorBody {
    match serve(net, &call).await {
        Ok(reply) => panic!("{call:?} должен вернуть ошибку, а вернул {reply:?}"),
        Err(error) => error.to_body(),
    }
}

fn body(code: &str, message: &str) -> ErrorBody {
    ErrorBody {
        code: String::from(code),
        message: String::from(message),
    }
}

async fn register(net: &Shared, raw: &str) -> UserId {
    match ok(
        net,
        Call::Register {
            username: name(raw),
        },
    )
    .await
    {
        Reply::User(user) => user.id,
        other => panic!("users.register отвечает Reply::User, а не {other:?}"),
    }
}

async fn post(net: &Shared, user: UserId, text: &str) -> PostId {
    match ok(
        net,
        Call::CreatePost {
            user,
            text: String::from(text),
        },
    )
    .await
    {
        Reply::Post(post) => post.id,
        other => panic!("posts.create отвечает Reply::Post, а не {other:?}"),
    }
}

fn line(id: u64, call: &Call) -> String {
    let mut out = Vec::new();
    write_line(&mut out, &Envelope::new(id, call.method(), call)).expect("запрос записывается");
    String::from_utf8(out).expect("запрос в UTF-8")
}

fn view(id: u64, author: UserId, author_name: &str, text: &str, likes: usize) -> PostView {
    PostView {
        id: PostId(id),
        author,
        author_name: String::from(author_name),
        text: String::from(text),
        likes,
    }
}

fn changed(changed: bool) -> Reply {
    Reply::Changed(Changed { changed })
}

#[tokio::test]
async fn serve_runs_every_call() {
    let net = shared();
    assert_eq!(
        ok(
            &net,
            Call::Register {
                username: name("alice")
            }
        )
        .await,
        Reply::User(UserView {
            id: UserId(1),
            username: String::from("alice"),
        }),
        "users.register отвечает id и ником"
    );
    let alice = UserId(1);
    let bob = register(&net, "bob").await;
    assert_eq!(
        ok(
            &net,
            Call::FindUser {
                username: name(" BOB ")
            }
        )
        .await,
        Reply::User(UserView {
            id: bob,
            username: String::from("bob"),
        }),
        "users.find ищет по нормализованному нику"
    );
    let follow = Call::Follow {
        user: bob,
        target: alice,
    };
    assert_eq!(
        ok(&net, follow.clone()).await,
        changed(true),
        "первая подписка"
    );
    assert_eq!(
        ok(&net, follow).await,
        changed(false),
        "повторная подписка ничего не меняет"
    );
    assert_eq!(
        ok(
            &net,
            Call::CreatePost {
                user: alice,
                text: String::from("  Привет #rust  ")
            }
        )
        .await,
        Reply::Post(view(1, alice, "alice", "Привет #rust", 0)),
        "posts.create отвечает созданным постом"
    );
    let like = Call::Like {
        user: bob,
        post: PostId(1),
    };
    assert_eq!(ok(&net, like.clone()).await, changed(true), "первый лайк");
    assert_eq!(
        ok(&net, like).await,
        changed(false),
        "второй лайк не считается"
    );
    assert_eq!(
        ok(
            &net,
            Call::Feed {
                user: bob,
                page: 1,
                size: 10
            }
        )
        .await,
        Reply::Feed(FeedPage {
            posts: vec![view(1, alice, "alice", "Привет #rust", 1)],
        }),
        "posts.feed отдаёт страницу ленты"
    );
    let sent = MessageView {
        from: bob,
        to: alice,
        text: String::from("привет"),
    };
    assert_eq!(
        ok(
            &net,
            Call::SendMessage {
                user: bob,
                peer: alice,
                text: String::from(" привет ")
            }
        )
        .await,
        Reply::Message(sent.clone()),
        "chat.send отвечает отправленным сообщением"
    );
    assert_eq!(
        ok(
            &net,
            Call::History {
                user: alice,
                peer: bob,
                limit: 20
            }
        )
        .await,
        Reply::History(ChatHistory {
            messages: vec![sent],
        }),
        "chat.history отдаёт переписку"
    );
    assert_eq!(
        ok(
            &net,
            Call::Mute {
                user: bob,
                target: alice
            }
        )
        .await,
        changed(true),
        "users.mute скрывает автора"
    );
    assert_eq!(
        ok(
            &net,
            Call::Feed {
                user: bob,
                page: 1,
                size: 10
            }
        )
        .await,
        Reply::Feed(FeedPage { posts: Vec::new() }),
        "скрытый автор пропал из ленты"
    );
    let delete = Call::DeletePost {
        user: alice,
        post: PostId(1),
    };
    assert_eq!(
        ok(&net, delete.clone()).await,
        changed(true),
        "автор удаляет пост"
    );
    assert_eq!(
        ok(&net, delete).await,
        changed(false),
        "второе удаление ничего не меняет"
    );
}

#[tokio::test]
async fn feed_pages_follow_the_adapter() {
    let net = shared();
    let alice = register(&net, "alice").await;
    for text in ["раз", "два", "три"] {
        post(&net, alice, text).await;
    }
    let page = async |page: usize, size: usize| match ok(
        &net,
        Call::Feed {
            user: alice,
            page,
            size,
        },
    )
    .await
    {
        Reply::Feed(feed) => feed
            .posts
            .iter()
            .map(|post| post.id.0)
            .collect::<Vec<u64>>(),
        other => panic!("posts.feed отвечает Reply::Feed, а не {other:?}"),
    };
    assert_eq!(
        page(1, 2).await,
        vec![3, 2],
        "первая страница: новые сверху"
    );
    assert_eq!(page(2, 2).await, vec![1], "последняя страница неполная");
    assert_eq!(page(3, 2).await, Vec::<u64>::new(), "за концом ленты пусто");
    assert_eq!(
        page(0, 2).await,
        Vec::<u64>::new(),
        "страницы нумеруются с 1"
    );
    assert_eq!(
        page(1, 0).await,
        Vec::<u64>::new(),
        "страниц размером 0 нет"
    );
    assert_eq!(
        page(usize::MAX, usize::MAX).await,
        Vec::<u64>::new(),
        "огромные номер и размер не роняют сервер"
    );
    assert_eq!(
        fail(
            &net,
            Call::Feed {
                user: UserId(9),
                page: 1,
                size: 10
            }
        )
        .await,
        body("not_found", "запрос отклонён: нет пользователя с id 9"),
        "лента незнакомого пользователя это not_found"
    );
}

#[tokio::test]
async fn domain_errors_get_codes() {
    let net = shared();
    let alice = register(&net, "alice").await;
    let bob = register(&net, "bob").await;
    let first = post(&net, alice, "первый").await;
    let cases = [
        (
            Call::Register {
                username: name("Alice"),
            },
            body("conflict", "запрос отклонён: ник alice уже занят"),
        ),
        (
            Call::FindUser {
                username: name("carol"),
            },
            body("not_found", "нет пользователя @carol"),
        ),
        (
            Call::Follow {
                user: alice,
                target: UserId(9),
            },
            body("not_found", "запрос отклонён: нет пользователя с id 9"),
        ),
        (
            Call::Follow {
                user: alice,
                target: alice,
            },
            body(
                "rejected",
                "запрос отклонён: нельзя подписаться на самого себя",
            ),
        ),
        (
            Call::CreatePost {
                user: alice,
                text: String::from("   "),
            },
            body("rejected", "запрос отклонён: пост не может быть пустым"),
        ),
        (
            Call::DeletePost {
                user: bob,
                post: first,
            },
            body(
                "rejected",
                "запрос отклонён: удалить пост [1] может только автор",
            ),
        ),
        (
            Call::Like {
                user: bob,
                post: PostId(7),
            },
            body("not_found", "запрос отклонён: нет поста с id 7"),
        ),
        (
            Call::SendMessage {
                user: bob,
                peer: bob,
                text: String::from("сам себе"),
            },
            body("rejected", "запрос отклонён: нельзя написать самому себе"),
        ),
        (
            Call::History {
                user: alice,
                peer: UserId(9),
                limit: 20,
            },
            body("not_found", "запрос отклонён: нет пользователя с id 9"),
        ),
    ];
    for (call, expected) in cases {
        let method = call.method();
        assert_eq!(
            fail(&net, call).await,
            expected,
            "код и текст ошибки {method}"
        );
    }
}

#[tokio::test]
async fn serve_line_answers_with_the_same_id() {
    let net = shared();
    let register = Call::Register {
        username: name("alice"),
    };
    assert_eq!(
        serve_line(&net, &line(7, &register)).await,
        Response {
            id: Some(7),
            outcome: Outcome::Ok(Reply::User(UserView {
                id: UserId(1),
                username: String::from("alice"),
            })),
        },
        "ответ несёт id запроса"
    );
    assert_eq!(
        serve_line(&net, &line(8, &register)).await,
        Response {
            id: Some(8),
            outcome: Outcome::Error(body("conflict", "запрос отклонён: ник alice уже занят")),
        },
        "отказ соцсети тоже несёт id запроса"
    );
}

#[tokio::test]
async fn broken_envelopes_get_a_null_id() {
    let net = shared();
    let cases = [
        (
            "{\"id\": 3",
            body("truncated", "запрос не разобран: конверт оборван"),
        ),
        (
            "nope",
            body(
                "syntax",
                "запрос не разобран: не JSON (строка 1, столбец 2)",
            ),
        ),
        (
            "{\"id\": 3, \"service\": \"users\", \"method\": \"users.find\"}",
            body(
                "bad_envelope",
                "запрос не разобран: кривой конверт: missing field `data`",
            ),
        ),
    ];
    for (raw, expected) in cases {
        assert_eq!(
            serve_line(&net, raw).await,
            Response {
                id: None,
                outcome: Outcome::Error(expected),
            },
            "конверт {raw:?} не разобран, id неизвестен"
        );
    }
}

#[tokio::test]
async fn second_stage_errors_keep_the_id() {
    let net = shared();
    let cases = [
        (
            r#"{"id": 4, "service": "users", "method": "users.fly", "data": {}}"#,
            body(
                "unknown_method",
                "запрос не разобран: неизвестный метод users.fly",
            ),
        ),
        (
            r#"{"id": 4, "service": "posts", "method": "users.find", "data": {"username": "a"}}"#,
            body(
                "wrong_service",
                "запрос не разобран: метод users.find не из сервиса posts",
            ),
        ),
        (
            r#"{"id": 4, "service": "users", "method": "users.follow", "data": {"user": "7", "target": "u_1"}}"#,
            body(
                "bad_data",
                "запрос не разобран: плохие данные для users.follow: неверный id \"7\": нужно u_<число>",
            ),
        ),
    ];
    for (raw, expected) in cases {
        assert_eq!(
            serve_line(&net, raw).await,
            Response {
                id: Some(4),
                outcome: Outcome::Error(expected),
            },
            "конверт {raw} разобран, ошибка во втором этапе"
        );
    }
}

fn wrap_contract() -> social_core::Result<u64> {
    let request: Envelope = contracts::decode("{")?;
    Ok(request.id)
}

fn wrap_domain() -> social_core::Result<()> {
    Err(DomainError::SelfFollow)?
}

fn into_box() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    wrap_domain()?;
    Ok(())
}

fn core_error<T: std::error::Error + Send + Sync + 'static>() {}

#[test]
fn crate_error_wraps_lower_layers() {
    core_error::<Error>();
    let contract = wrap_contract().expect_err("оборванный конверт это ошибка");
    assert!(
        matches!(contract, Error::Contract(contracts::Error::Truncated)),
        "From<contracts::Error> для social_core::Error: ? пересекает границу крейтов"
    );
    assert_eq!(
        contract.to_string(),
        "запрос не разобран",
        "Display говорит только о своём уровне"
    );
    assert_eq!(
        contract
            .source()
            .and_then(|cause| cause.downcast_ref::<contracts::Error>()),
        Some(&contracts::Error::Truncated),
        "source отдаёт ошибку контракта"
    );
    let domain = wrap_domain().expect_err("подписка на себя это ошибка");
    assert!(
        matches!(domain, Error::Domain(DomainError::SelfFollow)),
        "From<DomainError> для social_core::Error"
    );
    assert_eq!(
        domain.to_string(),
        "запрос отклонён",
        "Display говорит только о своём уровне"
    );
    assert_eq!(
        domain
            .source()
            .and_then(|cause| cause.downcast_ref::<DomainError>()),
        Some(&DomainError::SelfFollow),
        "source отдаёт доменную ошибку"
    );
    let boxed = into_box().expect_err("ошибка доезжает до коробки");
    assert_eq!(
        report(&*boxed),
        "запрос отклонён: нельзя подписаться на самого себя",
        "social_core::Error садится в Box<dyn Error + Send + Sync> через ?"
    );
    let unknown = Error::UnknownName(String::from("carol"));
    assert!(unknown.source().is_none(), "у UnknownName нет причины");
    let start = Error::Start(io::Error::other("нет потоков"));
    assert_eq!(start.code(), "internal", "Start это internal");
    assert_eq!(
        start.to_body(),
        body("internal", "не запустился поток уведомлений: нет потоков"),
        "to_body собирает текст всей цепочкой"
    );
}

#[derive(Debug)]
struct Layer {
    text: &'static str,
    cause: Option<Box<Layer>>,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl std::error::Error for Layer {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.cause {
            Some(cause) => Some(cause.as_ref()),
            None => None,
        }
    }
}

#[test]
fn report_walks_the_whole_chain() {
    let root = Layer {
        text: "диск полон",
        cause: None,
    };
    let middle = Layer {
        text: "журнал не записан",
        cause: Some(Box::new(root)),
    };
    let top = Layer {
        text: "пост не сохранён",
        cause: Some(Box::new(middle)),
    };
    assert_eq!(
        report(&top),
        "пост не сохранён: журнал не записан: диск полон",
        "report идёт по source до конца и склеивает через \": \""
    );
    assert_eq!(
        report(&Layer {
            text: "один",
            cause: None
        }),
        "один",
        "без причины report это просто Display"
    );
}

#[tokio::test]
async fn shared_network_reports_start_errors_as_core_errors() {
    let started: social_core::Result<Shared> = App::new(MemNetwork::new());
    assert!(started.is_ok(), "App::new возвращает social_core::Result");
    let with: social_core::Result<Shared> =
        App::with_notifiers(MemNetwork::new(), Notifiers::new());
    assert!(
        with.is_ok(),
        "App::with_notifiers возвращает social_core::Result"
    );
}

#[test]
fn notifications_become_pushes() {
    let cases = [
        (
            Notification::Followed { by: UserId(2) },
            Push::Followed {
                to: UserId(1),
                by: UserId(2),
            },
        ),
        (
            Notification::Liked {
                by: UserId(2),
                post: PostId(3),
            },
            Push::Liked {
                to: UserId(1),
                by: UserId(2),
                post: PostId(3),
            },
        ),
        (
            Notification::Message { from: UserId(2) },
            Push::Message {
                to: UserId(1),
                from: UserId(2),
            },
        ),
    ];
    for (notification, push) in cases {
        assert_eq!(
            notification.to_push(UserId(1)),
            push,
            "to_push для {notification:?}"
        );
    }
}

#[tokio::test]
async fn push_queue_collects_what_happened() {
    let pushes = Arc::new(PushQueue::new());
    let mut notifiers = Notifiers::new();
    notifiers.add(pushes.clone());
    let net = Shared::with_notifiers(MemNetwork::new(), notifiers)
        .expect("ядро запускается внутри рантайма");
    let alice = register(&net, "alice").await;
    let bob = register(&net, "bob").await;
    assert!(
        pushes.drain().is_empty(),
        "регистрация никого не уведомляет"
    );
    ok(
        &net,
        Call::Follow {
            user: bob,
            target: alice,
        },
    )
    .await;
    assert_eq!(
        pushes.drain(),
        vec![Push::Followed { to: alice, by: bob }],
        "подписка даёт push тому, на кого подписались"
    );
    assert!(pushes.drain().is_empty(), "drain забирает накопленное");
    let own = post(&net, alice, "мой пост").await;
    timeout(Duration::from_secs(10), net.flush())
        .await
        .expect("рассылка не уложилась в 10 с, похоже на зависание")
        .expect("рассылка доходит вовремя");
    assert_eq!(
        pushes.drain(),
        vec![Push::NewPost {
            to: bob,
            author: alice,
            post: own
        }],
        "подписчик bob получает push о новом посте, когда рассылка дошла (flush)"
    );
    ok(
        &net,
        Call::Like {
            user: alice,
            post: own,
        },
    )
    .await;
    assert!(pushes.drain().is_empty(), "свой лайк не уведомляет");
    ok(
        &net,
        Call::Like {
            user: bob,
            post: own,
        },
    )
    .await;
    ok(
        &net,
        Call::SendMessage {
            user: bob,
            peer: alice,
            text: String::from("привет"),
        },
    )
    .await;
    assert_eq!(
        pushes.drain(),
        vec![
            Push::Liked {
                to: alice,
                by: bob,
                post: own
            },
            Push::Message {
                to: alice,
                from: bob
            },
        ],
        "лайк и сообщение копятся в порядке событий"
    );
}
