use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use contracts::{COMMAND_NAMES, ParseError};
use social_core::{
    App, CommandBus, CommandError, Context, DomainError, Event, EventBus, IntoResponse,
    LogNotifier, MemNetwork, MemPostStore, MemUserStore, Notification, Notifier, Notifiers, PostId,
    Response, SubId, Topic, UserId, Visibility, execute_line, network_commands,
};
use tokio::runtime::Builder;
use tokio::task::JoinSet;
use tokio::time::timeout;

type Shared = App<MemUserStore, MemPostStore>;

const DEADLINE: Duration = Duration::from_secs(10);

async fn inbox(net: &Shared, user: UserId) -> Vec<Notification> {
    timeout(DEADLINE, net.inbox(user))
        .await
        .expect("inbox не уложился в 10 с, похоже на зависание")
        .expect("уведомления доставляются вовремя")
}
type Lines = Arc<Mutex<Vec<String>>>;

fn push(lines: &Lines, line: impl Into<String>) {
    lines
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(line.into());
}

fn seen(lines: &Lines) -> Vec<String> {
    lines.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

fn followed() -> Event {
    Event::Followed {
        who: UserId(1),
        whom: UserId(2),
    }
}

fn new_post() -> Event {
    Event::NewPost {
        author: UserId(1),
        post: PostId(1),
    }
}

fn sendable<T: Send>() {}

fn shareable<T: Send + Sync>() {}

#[derive(Default)]
struct Recorder {
    got: Mutex<Vec<(UserId, Notification)>>,
}

impl Recorder {
    fn got(&self) -> Vec<(UserId, Notification)> {
        self.got
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Notifier for Recorder {
    fn notify(&self, user: UserId, notification: Notification) {
        self.got
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((user, notification));
    }
}

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Buffer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner)).into_owned()
    }
}

impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn events_know_their_topic() {
    let cases = [
        (new_post(), Topic::NewPost),
        (
            Event::Liked {
                by: UserId(2),
                post: PostId(1),
                author: UserId(1),
            },
            Topic::Liked,
        ),
        (followed(), Topic::Followed),
        (
            Event::MessageSent {
                from: UserId(1),
                to: UserId(2),
            },
            Topic::MessageSent,
        ),
    ];
    for (event, topic) in cases {
        assert_eq!(event.topic(), topic, "тема события {event:?}");
    }
}

#[test]
fn bus_calls_subscribers_of_the_topic_in_order() {
    let mut bus = EventBus::new();
    let lines = Lines::default();
    let log = lines.clone();
    let first = bus.subscribe(Topic::Followed, move |_| push(&log, "first"));
    let log = lines.clone();
    let all = bus.subscribe(Topic::All, move |event| {
        push(&log, format!("all {:?}", event.topic()))
    });
    let log = lines.clone();
    let second = bus.subscribe(Topic::Followed, move |_| push(&log, "second"));
    let log = lines.clone();
    let posts = bus.subscribe(Topic::NewPost, move |_| push(&log, "posts"));
    assert_eq!(
        [first, all, second, posts],
        [SubId(1), SubId(2), SubId(3), SubId(4)],
        "id подписок начинаются с 1 и растут"
    );
    assert_eq!(
        bus.publish(&followed()),
        3,
        "publish возвращает, сколько обработчиков вызвано"
    );
    assert_eq!(
        seen(&lines),
        ["first", "all Followed", "second"],
        "вызываются подписчики темы и подписчики All, в порядке подписки"
    );
    assert_eq!(bus.subscribers(Topic::Followed), 3, "Followed услышат трое");
    assert_eq!(bus.subscribers(Topic::NewPost), 2, "NewPost услышат двое");
    assert_eq!(
        bus.subscribers(Topic::Liked),
        1,
        "Liked услышит только подписчик All"
    );
}

#[test]
fn unsubscribe_works_by_id() {
    let mut bus = EventBus::new();
    let lines = Lines::default();
    let log = lines.clone();
    let first = bus.subscribe(Topic::Followed, move |_| push(&log, "first"));
    let log = lines.clone();
    bus.subscribe(Topic::Followed, move |_| push(&log, "second"));
    assert!(bus.unsubscribe(first), "первая отписка находит подписку");
    assert!(
        !bus.unsubscribe(first),
        "повторная отписка ничего не находит"
    );
    assert!(
        !bus.unsubscribe(SubId(99)),
        "отписка по чужому id ничего не находит"
    );
    assert_eq!(bus.publish(&followed()), 1, "остался один подписчик");
    assert_eq!(
        seen(&lines),
        ["second"],
        "отписанный обработчик больше не вызывается"
    );
    assert_eq!(
        bus.subscribe(Topic::All, |_| {}),
        SubId(3),
        "id никогда не выдаются повторно, даже после отписки"
    );
}

#[test]
fn handlers_keep_state_between_calls() {
    let mut bus = EventBus::new();
    let total = Arc::new(AtomicUsize::new(0));
    let shown = Arc::clone(&total);
    let mut calls = 0;
    bus.subscribe(Topic::All, move |_| {
        calls += 1;
        shown.store(calls, Ordering::SeqCst);
    });
    for _ in 0..3 {
        bus.publish(&new_post());
    }
    assert_eq!(
        total.load(Ordering::SeqCst),
        3,
        "обработчик это FnMut: его собственный счётчик живёт между вызовами"
    );
}

#[test]
fn bus_goes_to_another_thread() {
    sendable::<EventBus>();
    let mut bus = EventBus::new();
    let total = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&total);
    bus.subscribe(Topic::All, move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let called = thread::scope(|scope| scope.spawn(move || bus.publish(&followed())).join());
    assert_eq!(
        called.ok(),
        Some(1),
        "шина вместе с обработчиками переезжает в другой поток и там работает"
    );
    assert_eq!(total.load(Ordering::SeqCst), 1, "обработчик вызван");
}

#[test]
fn network_publishes_domain_events() {
    let mut net = MemNetwork::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&events);
    net.subscribe(Topic::All, move |event| {
        log.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(*event)
    });
    let alice = net.register("alice").expect("alice регистрируется");
    let bob = net.register("bob").expect("bob регистрируется");
    let post = net
        .publish(alice, "первый пост", Visibility::Public)
        .expect("публикация проходит");
    assert_eq!(net.like(bob, post), Ok(true), "лайк проходит");
    assert_eq!(net.like(bob, post), Ok(false), "повторный лайк не засчитан");
    assert_eq!(net.like(alice, post), Ok(true), "свой пост лайкнуть можно");
    assert_eq!(net.follow(bob, alice), Ok(true), "подписка проходит");
    assert_eq!(
        net.follow(bob, alice),
        Ok(false),
        "повторная подписка ничего не меняет"
    );
    net.send_message(alice, bob, "привет")
        .expect("сообщение уходит");
    assert!(
        net.follow(alice, alice).is_err(),
        "на себя подписаться нельзя"
    );
    assert!(net.like(bob, PostId(99)).is_err(), "чужой пост не лайкнуть");
    assert!(
        net.send_message(alice, alice, "сам себе").is_err(),
        "себе не написать"
    );
    assert!(
        net.publish(alice, "   ", Visibility::Public).is_err(),
        "пустой пост не публикуется"
    );
    assert_eq!(
        *events.lock().unwrap_or_else(PoisonError::into_inner),
        [
            Event::NewPost {
                author: alice,
                post
            },
            Event::Liked {
                by: bob,
                post,
                author: alice
            },
            Event::Liked {
                by: alice,
                post,
                author: alice
            },
            Event::Followed {
                who: bob,
                whom: alice
            },
            Event::MessageSent {
                from: alice,
                to: bob
            },
        ],
        "событие публикуется только когда что-то изменилось: регистрация, повторы и ошибки событий не дают"
    );
}

#[test]
fn network_unsubscribe_stops_the_handler() {
    let mut net = MemNetwork::new();
    let alice = net.register("alice").expect("alice регистрируется");
    let total = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&total);
    let id = net.subscribe(Topic::NewPost, move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    net.publish(alice, "раз", Visibility::Public)
        .expect("публикация проходит");
    assert!(
        net.unsubscribe(id),
        "отписка через Network находит подписку"
    );
    net.publish(alice, "два", Visibility::Public)
        .expect("публикация проходит");
    assert_eq!(
        total.load(Ordering::SeqCst),
        1,
        "после отписки обработчик не вызывается"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_network_subscribes_from_any_task() {
    let net = Shared::new(MemNetwork::new()).expect("ядро запускается внутри рантайма");
    let total = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&total);
    let id = net.subscribe(Topic::NewPost, move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let mut writers = JoinSet::new();
    for writer in 0..4 {
        let net = net.clone();
        writers.spawn(async move {
            let me = net
                .register(&format!("writer{writer}"))
                .await
                .expect("у каждой задачи свой ник");
            for n in 0..10 {
                net.publish(me, &format!("пост {n}"), Visibility::Public)
                    .await
                    .expect("публикация проходит");
            }
        });
    }
    let finished = timeout(DEADLINE, async {
        while let Some(result) = writers.join_next().await {
            result.expect("задача публикации не падает");
        }
    })
    .await;
    assert!(
        finished.is_ok(),
        "четыре задачи по 10 постов: похоже на зависание"
    );
    assert_eq!(
        total.load(Ordering::SeqCst),
        40,
        "подписчик через App слышит посты из всех задач"
    );
    assert!(net.unsubscribe(id), "отписка через App работает");
}

#[test]
fn notifications_come_from_events() {
    let (alice, bob, post) = (UserId(1), UserId(2), PostId(7));
    let cases = [
        (
            Event::Followed {
                who: bob,
                whom: alice,
            },
            Some((alice, Notification::Followed { by: bob })),
        ),
        (
            Event::Liked {
                by: bob,
                post,
                author: alice,
            },
            Some((alice, Notification::Liked { by: bob, post })),
        ),
        (
            Event::Liked {
                by: alice,
                post,
                author: alice,
            },
            None,
        ),
        (
            Event::MessageSent {
                from: alice,
                to: bob,
            },
            Some((bob, Notification::Message { from: alice })),
        ),
        (
            Event::NewPost {
                author: alice,
                post,
            },
            None,
        ),
    ];
    for (event, expected) in cases {
        assert_eq!(
            Notification::for_event(&event),
            expected,
            "уведомление для {event:?}: кому и что (свой лайк и новый пост уведомлений не дают)"
        );
    }
}

#[test]
fn notifications_read_like_log_lines() {
    let texts = [
        (Notification::Followed { by: UserId(1) }, "подписка от id 1"),
        (
            Notification::Liked {
                by: UserId(1),
                post: PostId(3),
            },
            "лайк от id 1 на пост [3]",
        ),
        (
            Notification::Message { from: UserId(1) },
            "сообщение от id 1",
        ),
    ];
    for (notification, text) in texts {
        assert_eq!(
            notification.to_string(),
            text,
            "Display для {notification:?}"
        );
    }
}

#[tokio::test]
async fn chosen_notifiers_get_notifications() {
    shareable::<Arc<dyn Notifier>>();
    shareable::<Notifiers>();
    let recorder = Arc::new(Recorder::default());
    let mut notifiers = Notifiers::new();
    notifiers.add(recorder.clone());
    let net = Shared::with_notifiers(MemNetwork::new(), notifiers)
        .expect("ядро запускается внутри рантайма");
    let alice = net.register("alice").await.expect("alice регистрируется");
    let bob = net.register("bob").await.expect("bob регистрируется");
    let post = net
        .publish(alice, "пост", Visibility::Public)
        .await
        .expect("публикация проходит");
    net.follow(bob, alice).await.expect("подписка проходит");
    net.like(bob, post).await.expect("лайк проходит");
    net.like(alice, post).await.expect("свой лайк проходит");
    net.send_message(alice, bob, "привет")
        .await
        .expect("сообщение уходит");
    assert_eq!(
        recorder.got(),
        [
            (alice, Notification::Followed { by: bob }),
            (alice, Notification::Liked { by: bob, post }),
            (bob, Notification::Message { from: alice }),
        ],
        "выбранный при запуске уведомитель получает каждое уведомление по порядку"
    );
    assert_eq!(
        inbox(&net, alice).await,
        [
            Notification::Followed { by: bob },
            Notification::Liked { by: bob, post }
        ],
        "ящик пользователя остался одним из уведомителей и работает как раньше"
    );
    assert_eq!(
        inbox(&net, bob).await,
        [Notification::Message { from: alice }],
        "о личном сообщении получатель узнаёт из ящика"
    );
}

#[tokio::test]
async fn log_notifier_writes_a_line_per_notification() {
    let buffer = Buffer::default();
    let log = LogNotifier::new(buffer.clone());
    log.notify(UserId(2), Notification::Followed { by: UserId(1) });
    assert_eq!(
        buffer.text(),
        "[notify] id 2: подписка от id 1\n",
        "LogNotifier пишет строку `[notify] id <кому>: <уведомление>`"
    );
    let mut notifiers = Notifiers::new();
    notifiers.add(Arc::new(LogNotifier::new(buffer.clone())));
    let net = Shared::with_notifiers(MemNetwork::new(), notifiers)
        .expect("ядро запускается внутри рантайма");
    let alice = net.register("alice").await.expect("alice регистрируется");
    let bob = net.register("bob").await.expect("bob регистрируется");
    net.send_message(bob, alice, "привет")
        .await
        .expect("сообщение уходит");
    assert_eq!(
        buffer.text(),
        "[notify] id 2: подписка от id 1\n[notify] id 1: сообщение от id 2\n",
        "лог в реестре получает уведомления сети"
    );
}

#[tokio::test]
async fn inbox_command_shows_messages() {
    let net = Shared::new(MemNetwork::new()).expect("ядро запускается внутри рантайма");
    for line in ["register alice", "register bob"] {
        execute_line(line, &net)
            .await
            .expect("регистрация проходит");
    }
    let alice = UserId(1);
    let bob = UserId(2);
    net.send_message(alice, bob, "привет")
        .await
        .expect("сообщение уходит");
    assert_eq!(
        timeout(DEADLINE, execute_line("inbox bob", &net))
            .await
            .expect("inbox bob не уложился в 10 с"),
        Ok(String::from("@alice написал тебе")),
        "inbox печатает уведомление о сообщении"
    );
}

#[test]
fn anything_can_become_a_response() {
    let cases = [
        (String::from("готово").into_response(), (200, "готово")),
        ("готово".into_response(), (200, "готово")),
        (
            (201_u16, String::from("создано")).into_response(),
            (201, "создано"),
        ),
        (Response::new(204, "").into_response(), (204, "")),
        (
            Ok::<String, CommandError>(String::from("да")).into_response(),
            (200, "да"),
        ),
        (
            Err::<String, String>(String::from("упало")).into_response(),
            (500, "упало"),
        ),
        (
            Err::<String, (u16, String)>((418, String::from("чайник"))).into_response(),
            (418, "чайник"),
        ),
        (
            CommandError::Parse(ParseError::Empty).into_response(),
            (400, "пустая команда"),
        ),
        (
            CommandError::Parse(ParseError::UnknownCommand(String::from("dance"))).into_response(),
            (404, "неизвестная команда: dance"),
        ),
        (
            CommandError::UnknownName(String::from("ghost")).into_response(),
            (404, "нет пользователя @ghost"),
        ),
        (
            CommandError::Domain(DomainError::SelfFollow).into_response(),
            (422, "нельзя подписаться на самого себя"),
        ),
        (
            CommandError::NoExtension("u32").into_response(),
            (500, "в контексте нет u32"),
        ),
    ];
    for (response, (status, body)) in cases {
        assert_eq!(
            response,
            Response {
                status,
                body: String::from(body)
            },
            "IntoResponse: статус и тело"
        );
    }
    assert!(Response::new(200, "").is_ok(), "200 это успех");
    assert!(!Response::new(404, "").is_ok(), "от 400 и выше это ошибка");
}

#[test]
fn context_is_a_typed_map() {
    sendable::<Context>();
    let mut ctx = Context::new();
    assert_eq!(
        ctx.insert(7_u32),
        None,
        "первая вставка типа ничего не вытесняет"
    );
    assert_eq!(
        ctx.insert(8_u32),
        Some(7),
        "вставка того же типа отдаёт прежнее значение"
    );
    assert_eq!(
        ctx.insert(String::from("alice")),
        None,
        "у каждого типа своё место"
    );
    if let Some(name) = ctx.get_mut::<String>() {
        name.push('!');
    }
    assert_eq!(ctx.get::<u32>(), Some(&8), "get достаёт по типу");
    assert_eq!(
        ctx.get::<String>().map(String::as_str),
        Some("alice!"),
        "get_mut меняет значение на месте"
    );
    assert_eq!(ctx.get::<u64>(), None, "u64 никто не клал");
    assert_eq!(
        ctx.require::<u64>().err(),
        Some(CommandError::NoExtension("u64")),
        "require без значения даёт NoExtension с именем типа"
    );
    let recorder = Arc::new(Recorder::default());
    let chosen: Arc<dyn Notifier> = recorder.clone();
    ctx.insert(chosen);
    if let Some(notifier) = ctx.get::<Arc<dyn Notifier>>() {
        notifier.notify(UserId(1), Notification::Message { from: UserId(2) });
    }
    assert_eq!(
        recorder.got(),
        [(UserId(1), Notification::Message { from: UserId(2) })],
        "в контексте лежит уведомитель, выбранный при запуске, как Arc<dyn Notifier>"
    );
}

#[test]
fn bus_routes_lines_by_first_word() {
    shareable::<CommandBus>();
    let mut bus = CommandBus::new();
    assert!(
        !bus.register("echo", |_, line| String::from(line)),
        "новая команда ничего не заменяет"
    );
    assert!(
        bus.register("echo", |_, line| format!("эхо: {line}")),
        "та же команда второй раз заменяет обработчик"
    );
    bus.register(
        "count",
        |ctx: &mut Context, _: &str| -> Result<String, CommandError> {
            let count = ctx
                .get_mut::<u32>()
                .ok_or(CommandError::NoExtension("u32"))?;
            *count += 1;
            Ok(count.to_string())
        },
    );
    bus.register("teapot", |_, _| (418_u16, String::from("я чайник")));
    let mut ctx = Context::new();
    let cases = [
        ("  echo   раз  два ", 200, "эхо: echo   раз  два"),
        ("count", 500, "в контексте нет u32"),
        ("teapot", 418, "я чайник"),
        ("dance", 404, "неизвестная команда: dance"),
        ("   ", 400, "пустая команда"),
    ];
    for (line, status, body) in cases {
        assert_eq!(
            bus.dispatch(&mut ctx, line),
            Response::new(status, body),
            "dispatch {line:?}: обработчик получает строку без краёв, ответ любого типа становится Response"
        );
    }
    ctx.insert(0_u32);
    bus.dispatch(&mut ctx, "count");
    assert_eq!(
        bus.dispatch(&mut ctx, "count"),
        Response::new(200, "2"),
        "обработчики делят состояние через контекст"
    );
    assert_eq!(
        bus.names(),
        ["count", "echo", "teapot"],
        "names отдаёт имена по алфавиту"
    );
}

#[test]
fn network_commands_run_through_the_bus() {
    let mut bus = CommandBus::new();
    network_commands::<MemUserStore, MemPostStore>(&mut bus);
    let mut names = COMMAND_NAMES.to_vec();
    names.sort_unstable();
    assert_eq!(
        bus.names(),
        names,
        "network_commands регистрирует все сетевые команды"
    );
    let mut ctx = Context::new();
    let lost = bus.dispatch(&mut ctx, "register alice");
    assert_eq!(lost.status, 500, "без сети в контексте это ошибка 500");
    assert!(
        lost.body.starts_with("в контексте нет"),
        "тело говорит, чего не хватает: {}",
        lost.body
    );
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("рантайм для шины строится");
    let app = {
        let _entered = runtime.enter();
        Shared::new(MemNetwork::new()).expect("ядро запускается внутри рантайма")
    };
    ctx.insert(app);
    let cases = [
        ("register alice", 200, "зарегистрирован @alice (id 1)"),
        ("register ALICE", 422, "ник alice уже занят"),
        ("follow alice ghost", 404, "нет пользователя @ghost"),
        ("post alice", 400, "не хватает аргумента: текст поста"),
        ("post alice привет", 200, "опубликован пост [1]"),
        ("feed alice", 200, "@alice [1] привет (лайков: 0)"),
    ];
    for (line, status, body) in cases {
        assert_eq!(
            bus.dispatch(&mut ctx, line),
            Response::new(status, body),
            "ответ шины на {line:?}"
        );
    }
}
