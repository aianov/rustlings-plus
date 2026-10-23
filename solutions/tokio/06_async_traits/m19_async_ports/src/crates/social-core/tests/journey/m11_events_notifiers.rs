use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use contracts::ParseError;
use social_core::{
    App, CommandError, DomainError, Event, EventBus, IntoResponse, LogNotifier, MemNetwork,
    Notification, Notifier, Notifiers, PostId, Response, SubId, Topic, UserId, Visibility,
    execute_line,
};
use tokio::task::JoinSet;
use tokio::time::timeout;

type Shared = App;

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
async fn app_events_reach_a_subscriber_from_any_task() {
    let net = Shared::new(MemNetwork::new()).expect("ядро запускается внутри рантайма");
    let mut events = net.events();
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
    let mut posts = 0;
    while posts < 40 {
        let next = timeout(DEADLINE, events.next_event())
            .await
            .expect("событие приходит вовремя");
        match next.as_deref() {
            Some(Event::NewPost { .. }) => posts += 1,
            Some(_) => {}
            None => break,
        }
    }
    assert_eq!(posts, 40, "подписчик App слышит посты из всех задач");
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
            CommandError::NoRoute("users.card").into_response(),
            (500, "у метода users.card нет обработчика"),
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
