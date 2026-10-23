use std::future::Future;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant as WallClock};

use contracts::{ParseError, Setting};
use social_core::{
    App, AppConfig, AppError, CARD_POSTS, Card, CommandError, DomainError, Event, IntoResponse,
    MAX_POST_LEN, MemNetwork, Next, Notification, Notifier, Notifiers, PostId, Settings,
    TRENDS_WINDOW, UserId, Visibility, execute_line,
};
use test_support::FakeClock;
use tokio::time::timeout;

type Shared = App;

const DEADLINE: Duration = Duration::from_secs(10);

const START: u64 = 1_700_000_000;

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn start(config: AppConfig) -> Shared {
    App::start(MemNetwork::new(), Notifiers::new(), config)
        .expect("ядро запускается внутри рантайма")
}

fn start_at(clock: &FakeClock, config: AppConfig) -> Shared {
    App::start(
        MemNetwork::new().with_clock(clock.clone()),
        Notifiers::new(),
        config,
    )
    .expect("ядро запускается внутри рантайма")
}

async fn user(net: &Shared, name: &str) -> UserId {
    net.register(name)
        .await
        .unwrap_or_else(|e| panic!("{name} должен регистрироваться: {e}"))
}

async fn post(net: &Shared, author: UserId, text: &str) -> PostId {
    net.publish(author, text, Visibility::Public)
        .await
        .unwrap_or_else(|e| panic!("пост «{text}» должен публиковаться: {e}"))
}

async fn run(net: &Shared, line: &str) -> Result<String, CommandError> {
    within(line, execute_line(line, net)).await
}

async fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = WallClock::now() + Duration::from_secs(5);
    while !ready() {
        assert!(WallClock::now() < deadline, "за 5 с не дождались: {what}");
        tokio::task::yield_now().await;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn wait(&self) {
        let open = lock(&self.open);
        let _open = self
            .changed
            .wait_timeout_while(open, Duration::from_secs(5), |open| !*open)
            .unwrap_or_else(PoisonError::into_inner);
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
    delivered: Mutex<Vec<UserId>>,
}

impl Notifier for Held {
    fn notify(&self, user: UserId, notification: Notification) {
        if matches!(notification, Notification::NewPost { .. }) {
            self.gate.wait();
            lock(&self.delivered).push(user);
        }
    }
}

#[tokio::test]
async fn events_reach_every_subscriber() {
    let net = start(AppConfig::default());
    let mut first = net.events();
    let mut second = net.events();
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    let post = post(&net, alice, "всем").await;
    net.like(bob, post).await.expect("лайк проходит");
    let expected = [
        Event::Followed {
            who: bob,
            whom: alice,
        },
        Event::NewPost {
            author: alice,
            post,
        },
        Event::Liked {
            by: bob,
            post,
            author: alice,
        },
    ];
    for event in expected {
        let one = within("событие первому", first.next()).await;
        let two = within("событие второму", second.next()).await;
        let (Next::Event(one), Next::Event(two)) = (one, two) else {
            panic!("оба подписчика получают события по порядку, а пришло другое");
        };
        assert_eq!(*one, event, "события приходят в порядке изменений сети");
        assert!(
            Arc::ptr_eq(&one, &two),
            "broadcast раздаёт один Arc<Event> всем подписчикам, а не копию каждому"
        );
    }
}

#[tokio::test]
async fn slow_subscriber_counts_missed_events() {
    let net = start(AppConfig {
        events: 4,
        ..AppConfig::default()
    });
    let mut slow = net.events();
    let star = user(&net, "star").await;
    for n in 0..10 {
        let fan = user(&net, &format!("fan{n}")).await;
        net.follow(fan, star).await.expect("подписка проходит");
    }
    assert_eq!(
        within("первое событие", slow.next()).await,
        Next::Missed(6),
        "в буфере broadcast на 4 места помещаются последние 4 из 10 событий, о 6 подписчик узнаёт \
         как Missed(6)"
    );
    let mut rest = 0;
    while let Next::Event(event) = within("остаток", slow.next_or_closed()).await {
        assert!(matches!(*event, Event::Followed { .. }));
        rest += 1;
        if rest == 4 {
            break;
        }
    }
    assert_eq!(
        rest, 4,
        "после потери подписчик читает дальше то, что осталось"
    );
    assert_eq!(slow.missed(), 6, "EventStream считает пропущенное");
    wait_until(
        "обработчик трендов тоже отстал и посчитал потери",
        || net.missed_events() > 0,
    )
    .await;
}

trait NextOrClosed {
    fn next_or_closed(&mut self) -> impl Future<Output = Next>;
}

impl NextOrClosed for social_core::EventStream {
    fn next_or_closed(&mut self) -> impl Future<Output = Next> {
        self.next()
    }
}

#[tokio::test]
async fn stream_closes_when_the_app_is_gone() {
    let net = start(AppConfig::default());
    let mut stream = net.events();
    drop(net);
    assert_eq!(
        within("поток событий без ядра", stream.next()).await,
        Next::Closed,
        "последний App ушёл, отправитель broadcast вместе с ним: поток закрыт"
    );
}

#[tokio::test]
async fn delivery_keeps_order_for_each_follower() {
    let net = start(AppConfig::default());
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    let mut posts = Vec::new();
    for n in 0..20 {
        posts.push(post(&net, alice, &format!("пост {n}")).await);
    }
    let inbox = within("inbox bob", net.inbox(bob))
        .await
        .expect("рассылка доходит вовремя");
    let got: Vec<PostId> = inbox
        .iter()
        .filter_map(|n| match n {
            Notification::NewPost { post, .. } => Some(*post),
            _ => None,
        })
        .collect();
    assert_eq!(
        got, posts,
        "очередь доставки одна и упорядоченная: подписчик видит посты в порядке публикации"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_queue_holds_back_the_publisher() {
    let gate = Arc::new(Gate::default());
    let _opener = Opener(gate.clone());
    let held = Arc::new(Held {
        gate: gate.clone(),
        delivered: Mutex::new(Vec::new()),
    });
    let mut notifiers = Notifiers::new();
    notifiers.add(held.clone());
    let net = App::start(
        MemNetwork::new(),
        notifiers,
        AppConfig {
            queue: 2,
            ..AppConfig::default()
        },
    )
    .expect("ядро запускается внутри рантайма");
    let alice = user(&net, "alice").await;
    for n in 0..6 {
        let fan = user(&net, &format!("fan{n}")).await;
        net.follow(fan, alice).await.expect("подписка проходит");
    }
    let publishing = {
        let net = net.clone();
        tokio::spawn(async move {
            net.publish(alice, "шесть адресатов", Visibility::Public)
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !publishing.is_finished(),
        "очередь на 2 места, доставка висит: publish ждёт места в очереди (backpressure), а не \
         копит уведомления без предела"
    );
    gate.open();
    let published = within("publish после открытия", publishing).await;
    assert!(
        published.is_ok_and(|result| result.is_ok()),
        "когда доставка пошла, publish закончился"
    );
    within("flush", net.flush())
        .await
        .expect("рассылка доходит вовремя");
    assert_eq!(
        lock(&held.delivered).len(),
        6,
        "все шесть подписчиков получили пост"
    );
}

#[tokio::test]
async fn settings_change_live() {
    let net = start(AppConfig::default());
    assert_eq!(
        net.settings(),
        Settings {
            post_limit: MAX_POST_LEN,
            trends_window: TRENDS_WINDOW,
        },
        "по умолчанию предел поста MAX_POST_LEN и окно трендов сутки"
    );
    let mut watcher = net.watch_settings();
    let alice = user(&net, "alice").await;
    assert_eq!(
        net.set(Setting::PostLimit, 10),
        Ok(Settings {
            post_limit: 10,
            trends_window: TRENDS_WINDOW,
        })
    );
    within("подписчик watch видит изменение", watcher.changed())
        .await
        .expect("отправитель настроек жив");
    assert_eq!(watcher.borrow_and_update().post_limit, 10);
    assert_eq!(
        net.publish(alice, "одиннадцать", Visibility::Public).await,
        Err(AppError::Domain(DomainError::PostOverLimit(10))),
        "новый предел действует сразу: 11 символов при пределе 10"
    );
    assert!(
        net.publish(alice, "десять сим", Visibility::Public)
            .await
            .is_ok(),
        "10 символов при пределе 10 проходят"
    );
    for (setting, value) in [
        (Setting::PostLimit, 0),
        (Setting::PostLimit, MAX_POST_LEN as u64 + 1),
        (Setting::TrendsWindow, 0),
    ] {
        assert_eq!(
            net.set(setting, value),
            Err(AppError::BadSetting { setting, value }),
            "{setting} = {value} вне допустимого"
        );
    }
    assert_eq!(net.settings().post_limit, 10, "отказ не меняет настройку");
    assert_eq!(
        DomainError::PostOverLimit(10).to_string(),
        "пост длиннее 10 символов: такой предел сейчас поставил админ"
    );
}

#[tokio::test(start_paused = true)]
async fn trends_window_follows_the_setting() {
    let clock = FakeClock::new(START);
    let net = start_at(&clock, AppConfig::default());
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    let liked = post(&net, alice, "в тренды").await;
    net.like(bob, liked).await.expect("лайк проходит");
    within("пересчёт", net.refresh_trends())
        .await
        .expect("пересчёт проходит");
    assert_eq!(net.trends(5).len(), 1, "лайк в окне суток");
    let before = net.stats().trend_refreshes;
    net.set(Setting::TrendsWindow, 10).expect("окно 10 секунд");
    wait_until(
        "смена настройки сама будит пересчёт трендов",
        || net.stats().trend_refreshes > before,
    )
    .await;
    clock.advance(11);
    within("пересчёт", net.refresh_trends())
        .await
        .expect("пересчёт проходит");
    assert!(
        net.trends(5).is_empty(),
        "окно 10 секунд из настроек: через 11 секунд лайк уже не в трендах"
    );
}

#[tokio::test(start_paused = true)]
async fn likes_wake_the_trends_after_a_quiet_pause() {
    let net = start(AppConfig {
        trends_every: Duration::from_secs(600),
        trends_debounce: Duration::from_secs(2),
        ..AppConfig::default()
    });
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    let liked = post(&net, alice, "свежий").await;
    net.like(bob, liked).await.expect("лайк проходит");
    assert!(
        net.trends(5).is_empty(),
        "сразу после лайка сводку ещё не пересчитали"
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    wait_until("пересчёт после тишины в 2 с", || {
        net.stats().trend_refreshes >= 1
    })
    .await;
    assert_eq!(
        net.trends(5)
            .iter()
            .map(|trend| trend.post)
            .collect::<Vec<_>>(),
        [liked],
        "обработчик слышит Liked из шины и пересчитывает тренды через 2 с тишины, не дожидаясь тика"
    );
}

#[tokio::test]
async fn refresh_answers_through_oneshot() {
    let net = start(AppConfig::default());
    let before = net.stats().trend_refreshes;
    assert_eq!(
        within("refresh_trends", net.refresh_trends()).await,
        Ok(()),
        "обработчик трендов отвечает на запрос через oneshot"
    );
    assert_eq!(net.stats().trend_refreshes, before + 1);
}

#[tokio::test]
async fn card_collects_the_profile() {
    let net = start(AppConfig::default());
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    let carol = user(&net, "carol").await;
    let dave = user(&net, "dave").await;
    let mut posts = Vec::new();
    for n in 0..4 {
        posts.push(post(&net, alice, &format!("пост {n}")).await);
    }
    for fan in [bob, carol] {
        net.follow(fan, alice).await.expect("подписка проходит");
    }
    net.follow(alice, bob).await.expect("подписка проходит");
    for liked in &posts[..2] {
        net.like(bob, *liked).await.expect("лайк проходит");
    }
    let card = within("карточка", net.card(bob, alice))
        .await
        .expect("карточка собирается");
    assert_eq!(
        card,
        Card {
            id: alice,
            name: String::from("alice"),
            bio: String::new(),
            followers: 2,
            following: 1,
            posts: 4,
            likes: 2,
            recent: vec![posts[3], posts[2], posts[1]],
            followed: true,
        },
        "карточка: профиль, счётчики, {CARD_POSTS} последних поста и подписан ли смотрящий"
    );
    let stranger = within("карточка", net.card(dave, alice))
        .await
        .expect("карточка собирается");
    assert!(!stranger.followed, "dave на alice не подписан");
    assert_eq!(
        within("карточка", net.card(bob, UserId(99))).await,
        Err(AppError::Domain(DomainError::UnknownUser(UserId(99)))),
        "незнакомый пользователь: первая ошибка из try_join!"
    );
    assert_eq!(
        within("карточка", net.card(UserId(98), alice)).await,
        Err(AppError::Domain(DomainError::UnknownUser(UserId(98)))),
        "незнакомый смотрящий"
    );
}

#[tokio::test]
async fn card_and_set_commands() {
    let net = start(AppConfig::default());
    for line in [
        "register alice",
        "register bob",
        "follow bob alice",
        "post alice раз",
        "post alice два",
        "like bob 2",
    ] {
        run(&net, line).await.expect("подготовка проходит");
    }
    assert_eq!(
        run(&net, "card bob alice").await.as_deref(),
        Ok(
            "@alice\nподписчиков 1, подписок 0, постов 2, лайков 1\nты подписан\n[2] два (лайков: 1)\n[1] раз (лайков: 0)"
        ),
        "card <кто> <чей>: ник, счётчики, подписан ли смотрящий и последние посты"
    );
    assert_eq!(
        run(&net, "card alice alice").await.as_deref(),
        Ok(
            "@alice\nподписчиков 1, подписок 0, постов 2, лайков 1\nэто ты\n[2] два (лайков: 1)\n[1] раз (лайков: 0)"
        )
    );
    assert_eq!(
        run(&net, "card alice bob").await.as_deref(),
        Ok("@bob\nподписчиков 0, подписок 1, постов 0, лайков 0\nты не подписан")
    );
    assert_eq!(
        run(&net, "set post_limit 5").await.as_deref(),
        Ok("post_limit = 5"),
        "set <настройка> <значение> отвечает новым значением"
    );
    assert_eq!(
        run(&net, "post alice шесть!").await,
        Err(CommandError::Domain(DomainError::PostOverLimit(5)))
    );
    assert_eq!(
        run(&net, "set theme 1").await,
        Err(CommandError::Parse(ParseError::UnknownSetting(
            String::from("theme")
        )))
    );
    let bad = run(&net, "set trends_window 0")
        .await
        .expect_err("ноль секунд не окно");
    assert_eq!(bad.to_string(), "trends_window от 1 секунды, а не 0");
    assert_eq!(
        bad.into_response().status,
        422,
        "плохое значение настройки это 422"
    );
    assert_eq!(
        run(&net, "set trends_window 3600").await.as_deref(),
        Ok("trends_window = 3600")
    );
}
