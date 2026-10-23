use std::fmt::Debug;
use std::future::Future;
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use contracts::Push;
use social_core::{
    App, AppConfig, AppError, DomainError, MemNetwork, Notification, Notifier, Notifiers, PostId,
    Stats, UserId, Visibility, execute_line,
};
use test_support::SlowRepos;
use tokio::task::JoinSet;
use tokio::time::{Instant, timeout};

type Shared = App;

async fn within<T>(secs: u64, what: &str, work: impl Future<Output = T>) -> T {
    timeout(Duration::from_secs(secs), work)
        .await
        .unwrap_or_else(|_| panic!("{what}: нет ответа за {secs} с, похоже на зависание"))
}

async fn join_all<T: Send + 'static>(secs: u64, what: &str, mut tasks: JoinSet<T>) -> Vec<T> {
    within(secs, what, async move {
        let mut done = Vec::new();
        while let Some(result) = tasks.join_next().await {
            done.push(result.unwrap_or_else(|e| panic!("{what}: задача упала: {e}")));
        }
        done
    })
    .await
}

fn start(notifiers: Notifiers) -> Shared {
    App::with_notifiers(MemNetwork::new(), notifiers).expect("ядро запускается внутри рантайма")
}

fn with(notifier: Arc<dyn Notifier>) -> Shared {
    let mut notifiers = Notifiers::new();
    notifiers.add(notifier);
    start(notifiers)
}

async fn user(net: &Shared, name: &str) -> UserId {
    net.register(name).await.expect("новый ник регистрируется")
}

async fn fans(net: &Shared, star: UserId, count: usize) -> Vec<UserId> {
    let mut fans = Vec::new();
    for i in 0..count {
        let fan = user(net, &format!("fan{i}")).await;
        net.follow(fan, star).await.expect("подписка проходит");
        fans.push(fan);
    }
    fans
}

async fn post(net: &Shared, author: UserId, visibility: Visibility) -> PostId {
    net.publish(author, "новость", visibility)
        .await
        .expect("публикация проходит")
}

async fn flush(net: &Shared, secs: u64) {
    within(secs, "flush, ожидание рассылки", net.flush())
        .await
        .expect("рассылка укладывается в op_timeout");
}

fn new_post(author: UserId, post: PostId) -> Notification {
    Notification::NewPost { author, post }
}

fn sorted<T: Debug>(mut items: Vec<T>) -> Vec<T> {
    items.sort_by_key(|item| format!("{item:?}"));
    items
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct Recorder {
    got: Mutex<Vec<(UserId, Notification)>>,
}

impl Recorder {
    fn new_posts(&self) -> Vec<(UserId, Notification)> {
        lock(&self.got)
            .iter()
            .copied()
            .filter(|(_, notification)| matches!(notification, Notification::NewPost { .. }))
            .collect()
    }
}

impl Notifier for Recorder {
    fn notify(&self, user: UserId, notification: Notification) {
        lock(&self.got).push((user, notification));
    }
}

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn wait(&self) -> bool {
        let open = lock(&self.open);
        let (open, _) = self
            .changed
            .wait_timeout_while(open, Duration::from_secs(5), |open| !*open)
            .unwrap_or_else(PoisonError::into_inner);
        *open
    }

    fn open(&self) {
        *lock(&self.open) = true;
        self.changed.notify_all();
    }
}

struct Opener(Arc<Gate>);

impl Drop for Opener {
    fn drop(&mut self) {
        self.0.open();
    }
}

struct Held {
    gate: Arc<Gate>,
    inner: Recorder,
}

impl Notifier for Held {
    fn notify(&self, user: UserId, notification: Notification) {
        if matches!(notification, Notification::NewPost { .. }) {
            self.gate.wait();
        }
        self.inner.notify(user, notification);
    }
}

type Seen = Arc<Mutex<Vec<(Option<String>, Option<String>)>>>;

#[derive(Default)]
struct Curious {
    net: OnceLock<Shared>,
    seen: Seen,
}

impl Notifier for Curious {
    fn notify(&self, _user: UserId, notification: Notification) {
        let (Notification::NewPost { author, post }, Some(net)) = (notification, self.net.get())
        else {
            return;
        };
        let net = net.clone();
        let seen = Arc::clone(&self.seen);
        net.handle().clone().spawn(async move {
            let name = net
                .profile(author)
                .await
                .map(|user| user.name().to_string());
            let text = net.post(post).await.map(|post| post.text.to_string());
            lock(&seen).push((name, text));
        });
    }
}

#[test]
fn new_post_notification_has_text_and_push() {
    let notification = new_post(UserId(1), PostId(7));
    assert_eq!(
        notification.to_string(),
        "новый пост [7] от id 1",
        "Display нового уведомления в стиле остальных: что и от кого"
    );
    assert_eq!(
        notification.to_push(UserId(2)),
        Push::NewPost {
            to: UserId(2),
            author: UserId(1),
            post: PostId(7),
        },
        "to_push кладёт получателя в to, автора и пост в author и post"
    );
}

#[tokio::test]
async fn new_post_reaches_followers_who_may_see_it() {
    let recorder = Arc::new(Recorder::default());
    let net = with(recorder.clone());
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    let carol = user(&net, "carol").await;
    let dave = user(&net, "dave").await;
    user(&net, "eve").await;
    for fan in [bob, carol, dave] {
        net.follow(fan, alice).await.expect("подписка проходит");
    }
    net.mute(dave, alice).await.expect("скрыть автора можно");
    let open = post(&net, alice, Visibility::Public).await;
    let close = post(&net, alice, Visibility::Followers).await;
    post(&net, alice, Visibility::Private).await;
    flush(&net, 5).await;
    assert_eq!(
        sorted(recorder.new_posts()),
        sorted(vec![
            (bob, new_post(alice, open)),
            (carol, new_post(alice, open)),
            (bob, new_post(alice, close)),
            (carol, new_post(alice, close)),
        ]),
        "о новом посте узнают подписчики, которым он виден: публичный и для подписчиков да, \
         приватный никому; dave скрыл alice (mute), eve не подписана, им ничего"
    );
}

#[tokio::test]
async fn inbox_waits_for_the_fanout() {
    let net = start(Notifiers::new());
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    let first = post(&net, alice, Visibility::Public).await;
    let second = post(&net, alice, Visibility::Public).await;
    let inbox = within(5, "inbox после двух постов", net.inbox(bob))
        .await
        .expect("рассылка доходит вовремя");
    assert_eq!(
        sorted(inbox),
        sorted(vec![new_post(alice, first), new_post(alice, second)]),
        "inbox сначала дожидается рассылки: всё, что опубликовано до вызова, уже в ящике \
         (порядок двух постов не важен, их разносят разные задачи)"
    );
    let alice_inbox = within(5, "inbox автора", net.inbox(alice))
        .await
        .expect("рассылка доходит вовремя");
    assert_eq!(
        alice_inbox,
        vec![Notification::Followed { by: bob }],
        "автор о своих постах не узнаёт, у него только подписка bob"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn publish_does_not_wait_for_slow_delivery() {
    let gate = Arc::new(Gate::default());
    let _opener = Opener(gate.clone());
    let held = Arc::new(Held {
        gate: gate.clone(),
        inner: Recorder::default(),
    });
    let net = with(held.clone());
    let alice = user(&net, "alice").await;
    fans(&net, alice, 3).await;
    let published = within(
        2,
        "publish, пока доставка висит",
        net.publish(alice, "всем привет", Visibility::Public),
    )
    .await;
    assert!(
        published.is_ok(),
        "publish ставит рассылку задачами и сразу отвечает: {published:?}"
    );
    let late = within(2, "register, пока доставка висит", net.register("late")).await;
    assert_eq!(
        late,
        Ok(UserId(5)),
        "пока уведомления висят в задачах, сеть работает: доставка не держит замок сети"
    );
    assert_eq!(
        net.user_count().await,
        5,
        "читать сеть можно, пока доставка висит"
    );
    gate.open();
    flush(&net, 5).await;
    assert_eq!(
        held.inner.new_posts().len(),
        3,
        "когда доставка отвисла, все три подписчика получили уведомление"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn notifier_may_call_back_into_the_network() {
    let curious = Arc::new(Curious::default());
    let net = with(curious.clone());
    assert!(
        curious.net.set(net.clone()).is_ok(),
        "ручка сети кладётся в уведомитель один раз"
    );
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    let published = within(
        5,
        "publish с уведомителем, который читает сеть",
        net.publish(alice, "Привет из задачи", Visibility::Public),
    )
    .await;
    assert!(published.is_ok(), "публикация проходит: {published:?}");
    flush(&net, 5).await;
    within(5, "задача уведомителя", async {
        while lock(&curious.seen).is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert_eq!(
        *lock(&curious.seen),
        [(
            Some(String::from("alice")),
            Some(String::from("Привет из задачи"))
        )],
        "уведомитель синхронный, а ядро асинхронное: спросить ядро (профиль автора, текст поста) он \
         может своей задачей через handle().spawn, доставку это не держит"
    );
}

#[tokio::test]
async fn stats_count_posts_and_notifications() {
    let net = start(Notifiers::new());
    assert_eq!(
        net.stats(),
        Stats::default(),
        "у новой сети ноль постов и ноль уведомлений"
    );
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    let carol = user(&net, "carol").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    net.follow(carol, alice).await.expect("подписка проходит");
    let first = post(&net, alice, Visibility::Public).await;
    post(&net, alice, Visibility::Private).await;
    assert!(
        net.publish(alice, "   ", Visibility::Public).await.is_err(),
        "пустой пост не публикуется"
    );
    net.like(bob, first).await.expect("лайк проходит");
    net.send_message(bob, alice, "привет")
        .await
        .expect("сообщение уходит");
    flush(&net, 5).await;
    assert_eq!(
        net.stats(),
        Stats {
            posts: 2,
            notifications: 6,
            trend_refreshes: 0,
        },
        "постов два (пустой не считается); уведомлений шесть: две подписки, два новых поста \
         (приватный никому), лайк и сообщение"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stats_stay_exact_under_many_tasks() {
    let net = start(Notifiers::new());
    let mut authors = Vec::new();
    for i in 0..8 {
        let author = user(&net, &format!("author{i}")).await;
        let fan = user(&net, &format!("reader{i}")).await;
        net.follow(fan, author).await.expect("подписка проходит");
        authors.push(author);
    }
    let mut writers = JoinSet::new();
    for author in authors {
        let net = net.clone();
        writers.spawn(async move {
            for _ in 0..25 {
                post(&net, author, Visibility::Public).await;
            }
        });
    }
    join_all(20, "8 задач по 25 постов", writers).await;
    flush(&net, 5).await;
    assert_eq!(
        net.stats(),
        Stats {
            posts: 200,
            notifications: 208,
            trend_refreshes: 0,
        },
        "счётчики точные при любом числе задач: 200 постов, 8 подписок и 200 новых постов"
    );
}

#[tokio::test(start_paused = true)]
async fn stats_and_cached_profiles_skip_the_repositories() {
    let slow = SlowRepos::over(MemNetwork::new(), Duration::from_millis(100));
    let net = App::with_repos(slow.repos(), Notifiers::new(), AppConfig::default())
        .expect("ядро запускается внутри рантайма");
    let alice = user(&net, "alice").await;
    let cached = net.profile(alice).await.expect("профиль alice есть");
    let calls = slow.calls();
    let started = Instant::now();
    assert_eq!(
        net.stats(),
        Stats::default(),
        "stats читает атомарные счётчики"
    );
    let again = net.profile(alice).await.expect("профиль alice есть");
    assert!(
        Arc::ptr_eq(&again, &cached),
        "повторный профиль это тот же Arc из кэша"
    );
    assert_eq!(
        slow.calls(),
        calls,
        "ни stats, ни попадание в кэш профилей не ходят в хранилище"
    );
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "и поэтому не ждут его задержку ни миллисекунды"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profiles_are_cached_once() {
    let mut network = MemNetwork::new();
    let zoe = network.register("zoe").expect("ник свободен");
    let net = App::new(network).expect("ядро запускается внутри рантайма");
    let first = net
        .profile(zoe)
        .await
        .expect("пользователь, зарегистрированный до App, тоже находится");
    assert_eq!(first.name(), "zoe", "профиль берётся из сети");
    let second = net.profile(zoe).await.expect("профиль zoe есть");
    assert!(
        Arc::ptr_eq(&first, &second),
        "повторный запрос отдаёт тот же Arc из кэша, а не новую копию"
    );
    assert!(
        net.profile(UserId(2)).await.is_none(),
        "id 2 ещё никто не занял"
    );
    let bob = user(&net, "bob").await;
    assert_eq!(
        net.profile(bob).await.map(|user| user.name().to_string()),
        Some(String::from("bob")),
        "промах не кэшируется: после регистрации профиль находится"
    );
    let carol = user(&net, "carol").await;
    let mut readers = JoinSet::new();
    for _ in 0..8 {
        let net = net.clone();
        readers.spawn(async move { net.profile(carol).await });
    }
    let profiles: Vec<Arc<social_core::User>> =
        join_all(10, "8 одновременных промахов по одному профилю", readers)
            .await
            .into_iter()
            .flatten()
            .collect();
    assert_eq!(profiles.len(), 8, "все задачи нашли профиль carol");
    assert!(
        profiles
            .iter()
            .all(|profile| Arc::ptr_eq(profile, &profiles[0])),
        "одновременные промахи по одному профилю дают один Arc: вставляй через \
         entry().or_insert, а не insert поверх чужого"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_nickname_has_one_owner() {
    let net = start(Notifiers::new());
    let mut racers = JoinSet::new();
    for _ in 0..8 {
        let net = net.clone();
        racers.spawn(async move { net.register("same").await });
    }
    let results = join_all(10, "8 регистраций одного ника", racers).await;
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "из восьми одновременных регистраций одного ника проходит одна: проверка «ник свободен» \
         и вставка идут под одним захватом. Пришло {results:?}"
    );
    assert!(
        results.iter().all(|result| result.is_ok()
            || *result
                == Err(AppError::Domain(DomainError::NameTaken(String::from(
                    "same"
                ))))),
        "остальные получают NameTaken: {results:?}"
    );
    assert_eq!(
        net.register("next").await,
        Ok(UserId(2)),
        "проигравшие регистрации не сжигают id"
    );
    let mut writers = JoinSet::new();
    for task in 0..8 {
        let net = net.clone();
        writers.spawn(async move {
            let mut ids = Vec::new();
            for i in 0..10 {
                ids.push(user(&net, &format!("t{task}_{i}")).await.0);
            }
            ids
        });
    }
    let mut ids: Vec<u64> = join_all(10, "8 задач по 10 разных ников", writers)
        .await
        .into_iter()
        .flatten()
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        (3..=82).collect::<Vec<u64>>(),
        "80 одновременных регистраций разных ников получают id 3..=82 без дыр и повторов"
    );
}

#[tokio::test]
async fn commands_show_new_posts_and_stats() {
    let net = start(Notifiers::new());
    for line in [
        "register alice",
        "register bob",
        "follow bob alice",
        "post alice Привет",
    ] {
        execute_line(line, &net).await.expect("команда проходит");
    }
    let inbox = within(5, "inbox bob", execute_line("inbox bob", &net)).await;
    let stats = within(5, "stats", execute_line("stats", &net)).await;
    assert_eq!(
        inbox.as_deref(),
        Ok("@alice опубликовал пост [1]"),
        "inbox подписчика показывает новый пост с ником автора"
    );
    assert_eq!(
        stats.as_deref(),
        Ok("постов: 1, уведомлений: 2"),
        "stats дожидается рассылки и печатает счётчики: пост alice, подписка bob и новый пост"
    );
}
