use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Barrier, Condvar, Mutex, OnceLock, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use contracts::Push;
use social_core::{
    DomainError, FANOUT_WORKERS, MemNetwork, MemPostStore, MemUserStore, Notification, Notifier,
    Notifiers, PostId, SharedNetwork, Stats, UserId, Visibility, execute_line,
};

type Shared = SharedNetwork<MemUserStore, MemPostStore>;

const _: () = assert!(
    FANOUT_WORKERS >= 2,
    "FANOUT_WORKERS это размер пула рассылки, одного потока мало"
);

fn within<T: Send + 'static>(
    secs: u64,
    what: &str,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (done, result) = mpsc::channel();
    spawn(move || {
        let _ = done.send(work());
    });
    match result.recv_timeout(Duration::from_secs(secs)) {
        Ok(value) => value,
        Err(RecvTimeoutError::Timeout) => {
            panic!("{what}: нет ответа за {secs} с, похоже на зависание")
        }
        Err(RecvTimeoutError::Disconnected) => {
            panic!("{what}: поток теста упал, причина в сообщении выше")
        }
    }
}

fn spawn<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> JoinHandle<T> {
    thread::Builder::new()
        .spawn(work)
        .expect("ОС должна дать тесту поток")
}

fn start(notifiers: Notifiers) -> Shared {
    SharedNetwork::with_notifiers(MemNetwork::new(), notifiers)
        .expect("сеть, ящик и пул рассылки запускаются")
}

fn with(notifier: Arc<dyn Notifier>) -> Shared {
    let mut notifiers = Notifiers::new();
    notifiers.add(notifier);
    start(notifiers)
}

fn user(net: &Shared, name: &str) -> UserId {
    net.register(name).expect("новый ник регистрируется")
}

fn fans(net: &Shared, star: UserId, count: usize) -> Vec<UserId> {
    (0..count)
        .map(|i| {
            let fan = user(net, &format!("fan{i}"));
            net.follow(fan, star).expect("подписка проходит");
            fan
        })
        .collect()
}

fn post(net: &Shared, author: UserId, visibility: Visibility) -> PostId {
    net.publish(author, "новость", visibility)
        .expect("публикация проходит")
}

fn flush(net: &Shared, secs: u64) {
    let net = net.clone();
    within(secs, "flush, ожидание рассылки", move || net.flush());
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

#[derive(Default)]
struct Meeting {
    arrived: AtomicUsize,
    missed: AtomicBool,
}

impl Notifier for Meeting {
    fn notify(&self, _user: UserId, notification: Notification) {
        if !matches!(notification, Notification::NewPost { .. }) {
            return;
        }
        self.arrived.fetch_add(1, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.arrived.load(Ordering::SeqCst) < FANOUT_WORKERS {
            if Instant::now() > deadline {
                self.missed.store(true, Ordering::SeqCst);
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}

#[derive(Default)]
struct Curious {
    net: OnceLock<Shared>,
    seen: Mutex<Vec<(Option<String>, Option<String>)>>,
}

impl Notifier for Curious {
    fn notify(&self, _user: UserId, notification: Notification) {
        let (Notification::NewPost { author, post }, Some(net)) = (notification, self.net.get())
        else {
            return;
        };
        let name = net.profile(author).map(|user| user.name().to_string());
        let text = net.read(|network| network.post(post).map(|post| post.text().to_string()));
        lock(&self.seen).push((name, text));
    }
}

#[derive(Default)]
struct Slow {
    delivered: AtomicUsize,
}

impl Notifier for Slow {
    fn notify(&self, _user: UserId, notification: Notification) {
        if matches!(notification, Notification::NewPost { .. }) {
            thread::sleep(Duration::from_millis(5));
            self.delivered.fetch_add(1, Ordering::SeqCst);
        }
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

#[test]
fn new_post_reaches_followers_who_may_see_it() {
    let recorder = Arc::new(Recorder::default());
    let net = with(recorder.clone());
    let alice = user(&net, "alice");
    let bob = user(&net, "bob");
    let carol = user(&net, "carol");
    let dave = user(&net, "dave");
    user(&net, "eve");
    for fan in [bob, carol, dave] {
        net.follow(fan, alice).expect("подписка проходит");
    }
    net.mute(dave, alice).expect("скрыть автора можно");
    let open = post(&net, alice, Visibility::Public);
    let close = post(&net, alice, Visibility::Followers);
    post(&net, alice, Visibility::Private);
    flush(&net, 5);
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

#[test]
fn inbox_waits_for_the_fanout() {
    let net = start(Notifiers::new());
    let alice = user(&net, "alice");
    let bob = user(&net, "bob");
    net.follow(bob, alice).expect("подписка проходит");
    let first = post(&net, alice, Visibility::Public);
    let second = post(&net, alice, Visibility::Public);
    let inbox = {
        let net = net.clone();
        within(5, "inbox после двух постов", move || {
            net.inbox(bob)
        })
    };
    assert_eq!(
        sorted(inbox),
        sorted(vec![new_post(alice, first), new_post(alice, second)]),
        "inbox сначала дожидается рассылки: всё, что опубликовано до вызова, уже в ящике \
         (порядок двух постов не важен, их разносят разные воркеры)"
    );
    let alice_inbox = within(5, "inbox автора", move || net.inbox(alice));
    assert_eq!(
        alice_inbox,
        vec![Notification::Followed { by: bob }],
        "автор о своих постах не узнаёт, у него только подписка bob"
    );
}

#[test]
fn publish_does_not_wait_for_slow_delivery() {
    let gate = Arc::new(Gate::default());
    let _opener = Opener(gate.clone());
    let held = Arc::new(Held {
        gate: gate.clone(),
        inner: Recorder::default(),
    });
    let net = with(held.clone());
    let alice = user(&net, "alice");
    fans(&net, alice, 3);
    let published = {
        let net = net.clone();
        within(
            2,
            "publish, пока доставка висит",
            move || net.publish(alice, "всем привет", Visibility::Public),
        )
    };
    assert!(
        published.is_ok(),
        "publish ставит рассылку в пул и сразу отвечает: {published:?}"
    );
    let late = {
        let net = net.clone();
        within(
            2,
            "register, пока доставка висит",
            move || net.register("late"),
        )
    };
    assert_eq!(
        late,
        Ok(UserId(5)),
        "пока уведомления висят в пуле, сеть работает: доставка не держит замок сети"
    );
    let users = {
        let net = net.clone();
        within(
            2,
            "чтение сети, пока доставка висит",
            move || net.read(|network| network.user_count()),
        )
    };
    assert_eq!(users, 5, "читать сеть можно, пока доставка висит");
    gate.open();
    flush(&net, 5);
    assert_eq!(
        held.inner.new_posts().len(),
        3,
        "когда доставка отвисла, все три подписчика получили уведомление"
    );
}

#[test]
fn followers_are_notified_in_parallel() {
    let meeting = Arc::new(Meeting::default());
    let net = with(meeting.clone());
    let alice = user(&net, "alice");
    fans(&net, alice, FANOUT_WORKERS);
    post(&net, alice, Visibility::Public);
    flush(&net, 10);
    assert_eq!(
        meeting.arrived.load(Ordering::SeqCst),
        FANOUT_WORKERS,
        "каждый подписчик получил своё уведомление"
    );
    assert!(
        !meeting.missed.load(Ordering::SeqCst),
        "рассылка идёт в {FANOUT_WORKERS} потоков: уведомления {FANOUT_WORKERS} подписчикам \
         должны доставляться одновременно, а они шли по очереди. Каждому подписчику своё \
         задание в пуле на FANOUT_WORKERS воркеров"
    );
}

#[test]
fn notifier_may_call_back_into_the_network() {
    let curious = Arc::new(Curious::default());
    let net = with(curious.clone());
    assert!(
        curious.net.set(net.clone()).is_ok(),
        "ручка сети кладётся в уведомитель один раз"
    );
    let alice = user(&net, "alice");
    let bob = user(&net, "bob");
    net.follow(bob, alice).expect("подписка проходит");
    let published = {
        let net = net.clone();
        within(
            5,
            "publish с уведомителем, который читает сеть",
            move || net.publish(alice, "Привет из пула", Visibility::Public),
        )
    };
    assert!(published.is_ok(), "публикация проходит: {published:?}");
    flush(&net, 5);
    assert_eq!(
        *lock(&curious.seen),
        [(
            Some(String::from("alice")),
            Some(String::from("Привет из пула"))
        )],
        "уведомитель рассылки может сам спросить сеть (профиль автора, текст поста): доставка \
         идёт в пуле после того, как publish отпустил замок сети. Под замком Mutex заблокирует \
         сам себя"
    );
}

#[test]
fn stats_count_posts_and_notifications() {
    let net = start(Notifiers::new());
    assert_eq!(
        net.stats(),
        Stats::default(),
        "у новой сети ноль постов и ноль уведомлений"
    );
    let alice = user(&net, "alice");
    let bob = user(&net, "bob");
    let carol = user(&net, "carol");
    net.follow(bob, alice).expect("подписка проходит");
    net.follow(carol, alice).expect("подписка проходит");
    let first = post(&net, alice, Visibility::Public);
    post(&net, alice, Visibility::Private);
    assert!(
        net.publish(alice, "   ", Visibility::Public).is_err(),
        "пустой пост не публикуется"
    );
    net.like(bob, first).expect("лайк проходит");
    net.send_message(bob, alice, "привет")
        .expect("сообщение уходит");
    flush(&net, 5);
    assert_eq!(
        net.stats(),
        Stats {
            posts: 2,
            notifications: 6,
        },
        "постов два (пустой не считается); уведомлений шесть: две подписки, два новых поста \
         (приватный никому), лайк и сообщение"
    );
}

#[test]
fn stats_stay_exact_under_many_threads() {
    let net = start(Notifiers::new());
    let authors: Vec<UserId> = (0..8)
        .map(|i| {
            let author = user(&net, &format!("author{i}"));
            let fan = user(&net, &format!("reader{i}"));
            net.follow(fan, author).expect("подписка проходит");
            author
        })
        .collect();
    let stats = {
        let net = net.clone();
        within(20, "8 потоков по 25 постов", move || {
            let handles: Vec<JoinHandle<()>> = authors
                .into_iter()
                .map(|author| {
                    let net = net.clone();
                    spawn(move || {
                        for _ in 0..25 {
                            post(&net, author, Visibility::Public);
                        }
                    })
                })
                .collect();
            for handle in handles {
                handle.join().expect("поток публикаций не падает");
            }
            net.flush();
            net.stats()
        })
    };
    assert_eq!(
        stats,
        Stats {
            posts: 200,
            notifications: 208,
        },
        "счётчики точные при любом числе потоков: 200 постов, 8 подписок и 200 новых постов"
    );
}

#[test]
fn stats_and_cached_profiles_skip_the_network_lock() {
    let net = start(Notifiers::new());
    let alice = user(&net, "alice");
    let bob = user(&net, "bob");
    let cached = net.profile(alice).expect("профиль alice есть");
    let gate = Arc::new(Gate::default());
    let _opener = Opener(gate.clone());
    let (held_tx, held) = mpsc::channel();
    let holder = {
        let net = net.clone();
        let gate = gate.clone();
        spawn(move || {
            net.read(|_| {
                let _ = held_tx.send(());
                gate.wait()
            })
        })
    };
    held.recv_timeout(Duration::from_secs(5))
        .expect("поток теста должен взять замок сети");
    let loading = {
        let net = net.clone();
        spawn(move || net.profile(bob))
    };
    thread::sleep(Duration::from_millis(50));
    let stats = {
        let net = net.clone();
        within(2, "stats, пока замок сети занят", move || net.stats())
    };
    assert_eq!(
        stats,
        Stats::default(),
        "stats читает атомарные счётчики и не ждёт замок сети"
    );
    let again = {
        let net = net.clone();
        within(
            2,
            "профиль из кэша, пока замок сети занят",
            move || net.profile(alice),
        )
    };
    assert!(
        again.is_some_and(|again| Arc::ptr_eq(&again, &cached)),
        "профиль из кэша отдаётся под read-замком кэша без замка сети, даже пока рядом промах по \
         bob ждёт сеть: загрузка идёт без захвата кэша, write-замок берётся только на вставку"
    );
    gate.open();
    let loaded = within(5, "загрузка профиля bob", move || {
        loading.join()
    });
    let loaded = loaded.expect("поток загрузки профиля не падает");
    assert_eq!(
        loaded.map(|user| user.name().to_string()),
        Some(String::from("bob")),
        "промах кэша идёт в сеть, когда замок освободился"
    );
    let released = within(5, "поток, державший замок", move || {
        holder.join()
    });
    assert!(
        released.is_ok_and(|opened| opened),
        "держатель замка дождался открытия"
    );
}

#[test]
fn profiles_are_cached_once() {
    let mut network = MemNetwork::new();
    let zoe = network.register("zoe").expect("ник свободен");
    let net = SharedNetwork::new(network).expect("сеть запускается");
    let first = net
        .profile(zoe)
        .expect("пользователь, зарегистрированный до SharedNetwork, тоже находится");
    assert_eq!(first.name(), "zoe", "профиль берётся из сети");
    let second = net.profile(zoe).expect("профиль zoe есть");
    assert!(
        Arc::ptr_eq(&first, &second),
        "повторный запрос отдаёт тот же Arc из кэша, а не новую копию"
    );
    assert!(net.profile(UserId(2)).is_none(), "id 2 ещё никто не занял");
    let bob = net.register("bob").expect("ник свободен");
    assert_eq!(
        net.profile(bob).map(|user| user.name().to_string()),
        Some(String::from("bob")),
        "промах не кэшируется: после регистрации профиль находится"
    );
    let carol = net.register("carol").expect("ник свободен");
    let profiles = {
        let net = net.clone();
        within(
            10,
            "8 одновременных промахов по одному профилю",
            move || {
                let barrier = Arc::new(Barrier::new(8));
                let handles: Vec<JoinHandle<_>> = (0..8)
                    .map(|_| {
                        let net = net.clone();
                        let barrier = barrier.clone();
                        spawn(move || {
                            barrier.wait();
                            net.profile(carol)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().expect("поток профиля не падает"))
                    .collect::<Vec<_>>()
            },
        )
    };
    let profiles: Vec<Arc<social_core::User>> = profiles.into_iter().flatten().collect();
    assert_eq!(profiles.len(), 8, "все потоки нашли профиль carol");
    assert!(
        profiles
            .iter()
            .all(|profile| Arc::ptr_eq(profile, &profiles[0])),
        "одновременные промахи по одному профилю дают один Arc: вставляй через \
         entry().or_insert, а не insert поверх чужого"
    );
}

#[test]
fn one_nickname_has_one_owner() {
    let net = start(Notifiers::new());
    let results = {
        let net = net.clone();
        within(
            10,
            "8 регистраций одного ника",
            move || {
                let barrier = Arc::new(Barrier::new(8));
                let handles: Vec<JoinHandle<_>> = (0..8)
                    .map(|_| {
                        let net = net.clone();
                        let barrier = barrier.clone();
                        spawn(move || {
                            barrier.wait();
                            net.register("same")
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().expect("поток регистрации не падает"))
                    .collect::<Vec<_>>()
            },
        )
    };
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "из восьми одновременных регистраций одного ника проходит одна: проверка «ник свободен» \
         и вставка идут под одним захватом. Пришло {results:?}"
    );
    assert!(
        results
            .iter()
            .all(|result| result.is_ok()
                || *result == Err(DomainError::NameTaken(String::from("same")))),
        "остальные получают NameTaken: {results:?}"
    );
    assert_eq!(
        net.register("next"),
        Ok(UserId(2)),
        "проигравшие регистрации не сжигают id"
    );
    let ids = {
        let net = net.clone();
        within(
            10,
            "8 потоков по 10 разных ников",
            move || {
                let handles: Vec<JoinHandle<Vec<UserId>>> = (0..8)
                    .map(|thread| {
                        let net = net.clone();
                        spawn(move || {
                            (0..10)
                                .map(|i| user(&net, &format!("t{thread}_{i}")))
                                .collect()
                        })
                    })
                    .collect();
                let mut ids: Vec<u64> = handles
                    .into_iter()
                    .flat_map(|handle| handle.join().expect("поток регистрации не падает"))
                    .map(|id| id.0)
                    .collect();
                ids.sort_unstable();
                ids
            },
        )
    };
    assert_eq!(
        ids,
        (3..=82).collect::<Vec<u64>>(),
        "80 одновременных регистраций разных ников получают id 3..=82 без дыр и повторов"
    );
}

#[test]
fn dropping_the_network_delivers_the_queue() {
    let slow = Arc::new(Slow::default());
    let net = with(slow.clone());
    let alice = user(&net, "alice");
    fans(&net, alice, 40);
    post(&net, alice, Visibility::Public);
    within(5, "drop сети с очередью рассылки", move || drop(net));
    assert_eq!(
        slow.delivered.load(Ordering::SeqCst),
        40,
        "когда уходит последняя ручка SharedNetwork, пул рассылки останавливается мягко: \
         всё, что поставили в очередь, доставлено"
    );
}

#[test]
fn commands_show_new_posts_and_stats() {
    let net = start(Notifiers::new());
    for line in [
        "register alice",
        "register bob",
        "follow bob alice",
        "post alice Привет",
    ] {
        execute_line(line, &net).expect("команда проходит");
    }
    let (inbox, stats) = within(5, "inbox и stats", move || {
        (execute_line("inbox bob", &net), execute_line("stats", &net))
    });
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
