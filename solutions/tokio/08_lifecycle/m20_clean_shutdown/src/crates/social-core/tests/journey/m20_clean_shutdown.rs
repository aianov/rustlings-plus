use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use social_core::{
    App, AppConfig, AppError, MemNetwork, Notification, Notifier, Notifiers, Op, PostId, UserId,
    Visibility, execute_line,
};
use tokio::time::{Instant, sleep, timeout};

const DEADLINE: Duration = Duration::from_secs(10);

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn with(notifier: Arc<dyn Notifier>, config: AppConfig) -> App {
    let mut notifiers = Notifiers::new();
    notifiers.add(notifier);
    App::start(MemNetwork::new(), notifiers, config).expect("ядро запускается внутри рантайма")
}

async fn user(net: &App, name: &str) -> UserId {
    net.register(name)
        .await
        .unwrap_or_else(|e| panic!("{name} должен регистрироваться: {e}"))
}

async fn post(net: &App, author: UserId, text: &str) -> PostId {
    net.publish(author, text, Visibility::Public)
        .await
        .unwrap_or_else(|e| panic!("пост «{text}» должен публиковаться: {e}"))
}

#[derive(Default)]
struct Batches {
    sizes: Mutex<Vec<usize>>,
    seen: Mutex<Vec<(UserId, Notification)>>,
}

impl Notifier for Batches {
    fn notify(&self, user: UserId, notification: Notification) {
        lock(&self.sizes).push(1);
        lock(&self.seen).push((user, notification));
    }

    fn notify_batch(&self, batch: &[(UserId, Notification)]) {
        lock(&self.sizes).push(batch.len());
        lock(&self.seen).extend_from_slice(batch);
    }
}

impl Batches {
    fn new_posts(&self) -> Vec<(UserId, PostId)> {
        lock(&self.seen)
            .iter()
            .filter_map(|(user, notification)| match notification {
                Notification::NewPost { post, .. } => Some((*user, *post)),
                _ => None,
            })
            .collect()
    }
}

#[tokio::test(start_paused = true)]
async fn deliveries_go_in_batches_of_n() {
    let batches = Arc::new(Batches::default());
    let net = with(
        batches.clone(),
        AppConfig {
            delivery_workers: 1,
            batch_size: 4,
            batch_window: Duration::from_secs(3600),
            ..AppConfig::default()
        },
    );
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    lock(&batches.sizes).clear();
    for n in 0..8 {
        post(&net, alice, &format!("пост {n}")).await;
    }
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        *lock(&batches.sizes),
        [4, 4],
        "восемь уведомлений при batch_size = 4 уходят двумя пачками по четыре, не дожидаясь окна"
    );
}

#[tokio::test(start_paused = true)]
async fn partial_batch_leaves_after_the_window() {
    let batches = Arc::new(Batches::default());
    let net = with(
        batches.clone(),
        AppConfig {
            delivery_workers: 1,
            batch_size: 100,
            batch_window: Duration::from_millis(500),
            ..AppConfig::default()
        },
    );
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    lock(&batches.sizes).clear();
    let started = Instant::now();
    for n in 0..3 {
        post(&net, alice, &format!("пост {n}")).await;
    }
    sleep(Duration::from_millis(400)).await;
    assert!(
        batches.new_posts().is_empty(),
        "окно 500 мс ещё не прошло, а пачка из трёх не полная: доставка ждёт"
    );
    sleep(Duration::from_millis(200)).await;
    assert_eq!(
        *lock(&batches.sizes),
        [3],
        "по окну уходит неполная пачка целиком, через {:?}",
        started.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_follower_keeps_the_order_across_workers() {
    let batches = Arc::new(Batches::default());
    let net = with(
        batches.clone(),
        AppConfig {
            delivery_workers: 4,
            batch_size: 3,
            ..AppConfig::default()
        },
    );
    let alice = user(&net, "alice").await;
    let mut fans = Vec::new();
    for n in 0..12 {
        let fan = user(&net, &format!("fan{n}")).await;
        net.follow(fan, alice).await.expect("подписка проходит");
        fans.push(fan);
    }
    let mut posts = Vec::new();
    for n in 0..10 {
        posts.push(post(&net, alice, &format!("пост {n}")).await);
    }
    within("flush", net.flush())
        .await
        .expect("рассылка доходит вовремя");
    let seen = batches.new_posts();
    assert_eq!(
        seen.len(),
        120,
        "каждый из 12 подписчиков получил 10 постов"
    );
    for fan in fans {
        let got: Vec<PostId> = seen
            .iter()
            .filter(|(user, _)| *user == fan)
            .map(|(_, post)| *post)
            .collect();
        assert_eq!(
            got, posts,
            "у одного получателя посты в порядке публикации: его уведомления всегда в одном воркере"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn close_delivers_the_tail() {
    let batches = Arc::new(Batches::default());
    let net = with(
        batches.clone(),
        AppConfig {
            delivery_workers: 2,
            batch_size: 100,
            batch_window: Duration::from_secs(3600),
            ..AppConfig::default()
        },
    );
    let alice = user(&net, "alice").await;
    for n in 0..5 {
        let fan = user(&net, &format!("fan{n}")).await;
        net.follow(fan, alice).await.expect("подписка проходит");
    }
    post(&net, alice, "последний").await;
    tokio::task::yield_now().await;
    assert!(
        batches.new_posts().is_empty(),
        "пачка не полная и окно час: до остановки ничего не ушло"
    );
    let report = within("close", net.close(Duration::from_secs(5))).await;
    assert_eq!(
        report.aborted(),
        0,
        "всё остановилось само, никого не пришлось добивать"
    );
    assert_eq!(
        batches.new_posts().len(),
        5,
        "при остановке воркер дочитывает очередь и отдаёт неполную пачку, хвост не теряется"
    );
}

#[tokio::test]
async fn close_stops_in_order_and_the_core_refuses_work() {
    let net = App::new(MemNetwork::new()).expect("ядро запускается внутри рантайма");
    let alice = user(&net, "alice").await;
    let report = within("close", net.close(Duration::from_secs(5))).await;
    assert_eq!(
        report.order(),
        ["sources", "handlers", "trends"],
        "сначала источники работы, потом обработчики, последними тренды"
    );
    assert!(net.is_closed());
    assert_eq!(
        net.publish(alice, "после остановки", Visibility::Public)
            .await,
        Err(AppError::Stopped(Op::Flush)),
        "остановленное ядро не принимает посты"
    );
    assert_eq!(
        within("flush", net.flush()).await,
        Err(AppError::Stopped(Op::Flush))
    );
    assert_eq!(
        within("refresh", net.refresh_trends()).await,
        Err(AppError::Stopped(Op::Trends)),
        "актор трендов остановлен"
    );
}

struct Stuck {
    gate: Arc<AtomicBool>,
}

impl Notifier for Stuck {
    fn notify(&self, _user: UserId, notification: Notification) {
        if matches!(notification, Notification::NewPost { .. }) {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !self.gate.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                std::hint::spin_loop();
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_aborts_what_misses_the_deadline() {
    let gate = Arc::new(AtomicBool::new(false));
    let net = with(
        Arc::new(Stuck { gate: gate.clone() }),
        AppConfig {
            delivery_workers: 1,
            batch_size: 1,
            ..AppConfig::default()
        },
    );
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    post(&net, alice, "застрянет").await;
    let started = Instant::now();
    let report = within("close", net.close(Duration::from_millis(300))).await;
    gate.store(true, Ordering::SeqCst);
    assert!(
        report.aborted() >= 1,
        "воркер висит в уведомителе дольше дедлайна: close добивает его abort, а не ждёт: {report:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "close уложился в дедлайн, а не ждал воркер: {:?}",
        started.elapsed()
    );
}

struct PanicOnce {
    fired: AtomicBool,
    delivered: AtomicUsize,
}

impl Notifier for PanicOnce {
    fn notify(&self, _user: UserId, notification: Notification) {
        if matches!(notification, Notification::NewPost { .. }) {
            if !self.fired.swap(true, Ordering::SeqCst) {
                std::panic::resume_unwind(Box::new("уведомитель упал"));
            }
            self.delivered.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[tokio::test]
async fn supervisor_restarts_a_fallen_worker() {
    let notifier = Arc::new(PanicOnce {
        fired: AtomicBool::new(false),
        delivered: AtomicUsize::new(0),
    });
    let net = with(
        notifier.clone(),
        AppConfig {
            delivery_workers: 1,
            batch_size: 1,
            restart_pause: Duration::from_millis(10),
            ..AppConfig::default()
        },
    );
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    post(&net, alice, "уронит воркер").await;
    post(&net, alice, "дойдёт").await;
    within("flush", net.flush())
        .await
        .expect("после перезапуска доставка снова работает");
    assert_eq!(
        net.restarts(),
        1,
        "упавший воркер доставки перезапущен супервизором один раз"
    );
    assert_eq!(
        notifier.delivered.load(Ordering::SeqCst),
        1,
        "пост из упавшей пачки потерян, следующий доставлен"
    );
}

#[tokio::test]
async fn trends_live_in_an_actor() {
    let net = App::new(MemNetwork::new()).expect("ядро запускается внутри рантайма");
    let handle = net.trends_handle();
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    let liked = post(&net, alice, "в тренды").await;
    net.like(bob, liked).await.expect("лайк проходит");
    within("refresh", handle.refresh())
        .await
        .expect("актор пересчитывает тренды по запросу");
    let top = within("top", handle.top(5)).await.expect("актор отвечает");
    assert_eq!(
        top.iter().map(|trend| trend.post).collect::<Vec<_>>(),
        [liked],
        "TrendsHandle::top отвечает через oneshot"
    );
    assert_eq!(
        net.trends(5),
        top,
        "App::trends читает ту же сводку без запроса к актору"
    );
    assert_eq!(
        within("trends", execute_line("trends", &net))
            .await
            .as_deref(),
        Ok("@alice [1] в тренды (лайков: 1), за сутки +1")
    );
}
