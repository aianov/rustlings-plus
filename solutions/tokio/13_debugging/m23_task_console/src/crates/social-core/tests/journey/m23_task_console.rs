use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use contracts::Command;
use futures::StreamExt;
use social_core::{
    App, AppConfig, Journal, MemNetwork, MemRepo, Notification, Notifier, Notifiers, UserId,
    Visibility, social_table,
};
use test_support::Probe;
use tokio::time::{sleep, timeout};

const DEADLINE: Duration = Duration::from_secs(10);

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn start(config: AppConfig, notifiers: Notifiers) -> App {
    App::start(MemNetwork::new(), notifiers, config).expect("ядро запускается в рантайме")
}

fn names(app: &App) -> BTreeMap<&'static str, usize> {
    let mut names = BTreeMap::new();
    for task in app.console().tasks() {
        *names.entry(task.name).or_default() += 1;
    }
    names
}

async fn flushed(app: &App) {
    within("доставка", app.flush())
        .await
        .expect("доставка уведомлений проходит");
}

async fn settles_at(app: &App, alive: usize, what: &str) {
    let settled = timeout(DEADLINE, async {
        while app.console().alive() != alive {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        settled.is_ok(),
        "{what}: живых задач {} вместо {alive}, кто-то утёк:\n{}",
        app.console().alive(),
        app.console().dump()
    );
}

fn scratch(name: &str, text: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m23");
    std::fs::create_dir_all(&dir).expect("папка для файлов теста создаётся");
    let path = dir.join(name);
    std::fs::write(&path, text).expect("файл импорта пишется");
    path
}

#[tokio::test]
async fn every_core_task_is_in_the_console() {
    let app = start(AppConfig::default(), Notifiers::new());
    flushed(&app).await;
    assert_eq!(
        names(&app),
        BTreeMap::from([
            ("fanout-supervisor", 4),
            ("fanout-worker", 4),
            ("trends-actor", 1),
            ("trends-ticker", 1),
        ]),
        "каждая фоновая задача ядра запущена через консоль под своим именем: тикер трендов, актор трендов, четыре супервизора доставки и их воркеры"
    );
    let journaled = App::journaled(
        MemRepo::repos(MemNetwork::new()),
        Notifiers::new(),
        AppConfig {
            delivery_workers: 2,
            ..AppConfig::default()
        },
        Journal::new(Probe::new()),
    )
    .expect("ядро с журналом запускается");
    flushed(&journaled).await;
    assert_eq!(
        names(&journaled).get("journal-writer"),
        Some(&1),
        "у ядра с журналом есть задача journal-writer"
    );
    assert_eq!(
        names(&journaled).get("fanout-worker"),
        Some(&2),
        "воркеров доставки столько, сколько delivery_workers"
    );
    within("остановка", app.close(DEADLINE)).await;
    within("остановка", journaled.close(DEADLINE)).await;
    assert_eq!(
        (app.console().alive(), journaled.console().alive()),
        (0, 0),
        "после close в консоли не остаётся живых задач:\n{}\n{}",
        app.console().dump(),
        journaled.console().dump()
    );
    assert_eq!(
        app.console().counts().panicked,
        0,
        "при остановке ничего не паникует"
    );
}

#[tokio::test]
async fn a_busy_session_leaves_no_tasks_behind() {
    let app = start(AppConfig::default(), Notifiers::new());
    flushed(&app).await;
    let baseline = app.console().alive();
    let before = app.console().counts();

    let mut seed = String::new();
    for n in 0..10 {
        seed.push_str(&format!("register user_{n}\n"));
    }
    for n in 1..10 {
        seed.push_str(&format!("follow user_{n} user_0\n"));
    }
    seed.push_str("post user_0 Привет всем\npost user_1 И тебе\n");
    within("импорт", app.import_file(scratch("seed.txt", &seed)))
        .await
        .unwrap_or_else(|e| panic!("импорт проходит: {e:?}"));
    let broken = scratch("broken.txt", "register a\nregister b\nfollow a nobody\n");
    assert!(
        within("импорт с ошибкой", app.import_file(broken))
            .await
            .is_err(),
        "подписка на неизвестного это ошибка импорта"
    );

    let author = UserId(1);
    let reader = UserId(2);
    let post = within(
        "пост",
        app.publish(author, "Живой пост", Visibility::Public),
    )
    .await
    .expect("пост публикуется");
    within("лайк", app.like(reader, post))
        .await
        .expect("лайк проходит");
    within("карточка", app.card(reader, author))
        .await
        .expect("карточка собирается");
    let mut watch = Box::pin(app.watch(reader, 3));
    while let Some(event) = within("живая лента", watch.next()).await {
        if event.is_ok_and(|event| event == social_core::FeedEvent::UpToDate) {
            break;
        }
    }
    drop(watch);
    flushed(&app).await;

    let after = app.console().counts();
    assert!(
        after.spawned >= before.spawned + 10,
        "задачи импорта тоже запускаются через консоль: запущено было {}, стало {}",
        before.spawned,
        after.spawned
    );
    assert_eq!(after.panicked, 0, "за сеанс ничего не упало");
    settles_at(&app, baseline, "после сеанса").await;
    within("остановка", app.close(DEADLINE)).await;
    assert_eq!(
        app.console().alive(),
        0,
        "после close живых задач нет:\n{}",
        app.console().dump()
    );
}

fn spin(busy: Duration) {
    let started = std::time::Instant::now();
    while started.elapsed() < busy {
        std::hint::spin_loop();
    }
}

struct Sluggish;

impl Notifier for Sluggish {
    fn notify(&self, _user: UserId, notification: Notification) {
        if matches!(notification, Notification::NewPost { .. }) {
            spin(Duration::from_millis(60));
        }
    }
}

#[tokio::test]
async fn blocking_notifier_shows_up_as_a_long_poll() {
    let mut notifiers = Notifiers::new();
    notifiers.add(Arc::new(Sluggish));
    let app = start(
        AppConfig {
            long_poll: Duration::from_millis(20),
            ..AppConfig::default()
        },
        notifiers,
    );
    let alice = within("регистрация", app.register("alice"))
        .await
        .expect("alice регистрируется");
    let bob = within("регистрация", app.register("bob"))
        .await
        .expect("bob регистрируется");
    within("подписка", app.follow(bob, alice))
        .await
        .expect("подписка проходит");
    within("пост", app.publish(alice, "Медленно", Visibility::Public))
        .await
        .expect("пост публикуется");
    flushed(&app).await;
    let long = app.console().long_polls();
    let slow = long
        .iter()
        .find(|poll| poll.name == "fanout-worker")
        .unwrap_or_else(|| {
            panic!("уведомитель крутится 60 мс без .await прямо в poll воркера доставки, сторож обязан это заметить: {long:?}")
        });
    assert!(
        slow.took >= Duration::from_millis(60),
        "сторож меряет длину poll: {:?}",
        slow.took
    );
    let dump = within("команда tasks", social_table().call(&app, Command::Tasks))
        .await
        .unwrap_or_else(|e| panic!("команда tasks отвечает: {e:?}"));
    assert!(
        dump.starts_with("задач живых 10, "),
        "дамп начинается со счётчиков, живых задач десять:\n{dump}"
    );
    assert!(
        dump.contains(&format!("долгий poll: #{} fanout-worker ", slow.id)),
        "дамп показывает долгие poll:\n{dump}"
    );
    within("остановка", app.close(DEADLINE)).await;
}
