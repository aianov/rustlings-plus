use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use social_core::{
    Clock, MemNetwork, Network, Notification, Notifier, Post, PostId, PostStore, User, UserId,
    UserStore, Visibility,
};
use test_support::{
    CountingAlloc, FakeClock, FakePosts, FakeUsers, RecordingNotifier, SeqIds, Summary,
    allocations, measure,
};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

const DEADLINE: Duration = Duration::from_secs(10);

fn within<T: Send + 'static>(what: &str, job: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, result) = mpsc::channel();
    thread::Builder::new()
        .name(String::from("m16-test"))
        .spawn(move || {
            let _ = done.send(job());
        })
        .expect("поток теста запускается");
    match result.recv_timeout(DEADLINE) {
        Ok(value) => value,
        Err(RecvTimeoutError::Timeout) => {
            panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание")
        }
        Err(RecvTimeoutError::Disconnected) => panic!("{what}: поток теста упал с паникой"),
    }
}

#[test]
fn fake_clock_stands_still_until_moved_by_hand() {
    let clock = FakeClock::new(1_700_000_000);
    assert_eq!(clock.now(), 1_700_000_000, "FakeClock::new(t) стоит на t");
    assert_eq!(clock.now(), 1_700_000_000, "сами часы не идут");
    clock.advance(90);
    assert_eq!(
        clock.now(),
        1_700_000_090,
        "advance(90) двигает часы на 90 секунд"
    );
    clock.set(42);
    assert_eq!(
        clock.now(),
        42,
        "set(t) ставит часы ровно на t, хоть и назад"
    );
}

#[test]
fn fake_clock_clones_share_one_time() {
    let clock = FakeClock::new(100);
    let handle = clock.clone();
    let inside: Box<dyn Clock> = Box::new(clock.clone());
    handle.advance(5);
    assert_eq!(
        clock.now(),
        105,
        "клон FakeClock это та же стрелка: тест двигает свой клон, а сеть видит новое время"
    );
    assert_eq!(inside.now(), 105, "FakeClock работает как Box<dyn Clock>");
}

#[test]
fn seq_ids_start_where_told() {
    let ids = SeqIds::starting_at(42);
    let got: Vec<u64> = (0..3).map(|_| ids.next_id()).collect();
    assert_eq!(
        got,
        [42, 43, 44],
        "SeqIds::starting_at(42) выдаёт 42, 43, 44"
    );
}

#[test]
fn seq_ids_never_repeat_across_threads() {
    let ids = Arc::new(SeqIds::starting_at(1));
    let got = within("8 потоков по 100 id", move || {
        let (sent, all) = mpsc::channel();
        for n in 0..8 {
            let ids = Arc::clone(&ids);
            let sent = sent.clone();
            thread::Builder::new()
                .name(format!("ids-{n}"))
                .spawn(move || {
                    let mine: Vec<u64> = (0..100).map(|_| ids.next_id()).collect();
                    let _ = sent.send(mine);
                })
                .expect("поток запускается");
        }
        drop(sent);
        let mut got: Vec<u64> = all.iter().flatten().collect();
        got.sort_unstable();
        got
    });
    let expected: Vec<u64> = (1..=800).collect();
    assert_eq!(
        got, expected,
        "SeqIds из многих потоков не повторяет и не теряет id"
    );
}

#[test]
fn fake_users_hand_out_ids_from_the_given_start() {
    let mut users = FakeUsers::starting_at(100);
    let first = users.next_id();
    let second = users.next_id();
    assert_eq!((first, second), (UserId(100), UserId(101)));
    users.insert(User::new(first, String::from("alice")));
    assert_eq!(users.count(), 1);
    assert_eq!(users.id_by_name("alice"), Some(first));
    assert_eq!(users.get(first).map(User::name), Some("alice"));
    assert!(
        users.get(second).is_none(),
        "выданный, но не вставленный id не найден"
    );
}

#[test]
fn fake_posts_hand_out_ids_from_the_given_start() {
    let mut posts = FakePosts::starting_at(500);
    let id = posts.next_id();
    assert_eq!(id, PostId(500));
    posts.insert(Post::new(
        id,
        UserId(1),
        String::from("привет"),
        Visibility::Public,
    ));
    assert_eq!(posts.get(id).map(Post::text), Some("привет"));
    assert!(
        posts.get_mut(id).is_some_and(|post| post.like(UserId(2))),
        "get_mut отдаёт тот же пост"
    );
    assert_eq!(posts.get(id).map(Post::likes), Some(1));
    assert_eq!(posts.count(), 1);
    assert_eq!(posts.next_id(), PostId(501));
}

#[test]
fn fake_posts_count_full_scans() {
    let mut posts = FakePosts::starting_at(1);
    let scans = posts.scans();
    let id = posts.next_id();
    posts.insert(Post::new(
        id,
        UserId(1),
        String::from("раз"),
        Visibility::Public,
    ));
    let _ = posts.get(id);
    let _ = posts.count();
    assert_eq!(
        scans.get(),
        0,
        "get, insert и count не обходят хранилище целиком"
    );
    let _ = posts.all();
    let _ = posts.all();
    assert_eq!(scans.get(), 2, "каждый all() это один полный обход");
    assert_eq!(scans.clone().get(), 2, "клон счётчика видит те же обходы");
}

#[test]
fn fake_stores_drive_a_real_network() {
    let mut net = Network::with_stores(FakeUsers::starting_at(1000), FakePosts::starting_at(7000));
    let alice = net.register("alice").expect("alice регистрируется");
    let bob = net.register("bob").expect("bob регистрируется");
    assert_eq!((alice, bob), (UserId(1000), UserId(1001)));
    net.follow(bob, alice).expect("подписка проходит");
    let post = net
        .publish(alice, "с фейками", Visibility::Followers)
        .expect("пост публикуется");
    assert_eq!(post, PostId(7000));
    let feed: Vec<PostId> = net.feed(bob).map(Post::id).collect();
    assert_eq!(
        feed,
        [post],
        "сеть на фейках работает так же, как на памяти"
    );
}

#[test]
fn recording_notifier_keeps_calls_in_order() {
    let log = RecordingNotifier::new();
    let followed = Notification::Followed { by: UserId(2) };
    let liked = Notification::Liked {
        by: UserId(1),
        post: PostId(7),
    };
    log.notify(UserId(1), followed);
    log.notify(UserId(2), liked);
    log.notify(UserId(1), Notification::Message { from: UserId(2) });
    assert_eq!(log.count(), 3);
    assert_eq!(
        log.calls(),
        [
            (UserId(1), followed),
            (UserId(2), liked),
            (UserId(1), Notification::Message { from: UserId(2) }),
        ],
        "журнал хранит вызовы в порядке поступления"
    );
    assert_eq!(
        log.for_user(UserId(1)),
        [followed, Notification::Message { from: UserId(2) }],
        "for_user отдаёт уведомления одного получателя"
    );
    assert!(log.for_user(UserId(9)).is_empty());
}

#[test]
fn recording_notifier_is_shared_between_threads() {
    let log = Arc::new(RecordingNotifier::new());
    let shared: Arc<dyn Notifier> = log.clone();
    within(
        "4 потока по 100 уведомлений",
        move || {
            let (sent, finished) = mpsc::channel();
            for n in 0..4 {
                let notifier = Arc::clone(&shared);
                let sent = sent.clone();
                thread::Builder::new()
                    .name(format!("notify-{n}"))
                    .spawn(move || {
                        for _ in 0..100 {
                            notifier.notify(UserId(n), Notification::Followed { by: UserId(9) });
                        }
                        let _ = sent.send(());
                    })
                    .expect("поток запускается");
            }
            drop(sent);
            finished.iter().count()
        },
    );
    assert_eq!(
        log.count(),
        400,
        "журнал не теряет вызовы из разных потоков"
    );
    assert_eq!(log.for_user(UserId(3)).len(), 100);
}

#[test]
fn allocations_counts_heap_work() {
    let (_, none) = allocations(|| 6 * 7);
    assert_eq!(none, 0, "сложение чисел ничего не просит у кучи");
    let (buffer, one) = allocations(|| Vec::<u8>::with_capacity(64));
    assert_eq!(
        one, 1,
        "Vec::with_capacity это ровно одна аллокация. Если тут 0, CountingAlloc не считает alloc"
    );
    drop(buffer);
    let (text, grown) = allocations(|| {
        let mut text = String::new();
        for _ in 0..100 {
            text.push('x');
        }
        text
    });
    assert_eq!(text.len(), 100);
    assert!(
        grown > 1,
        "строка, растущая с нуля до 100 байт, переаллоцируется несколько раз, а насчитано {grown}: realloc тоже аллокация"
    );
}

#[test]
fn allocations_counts_only_its_own_thread() {
    let (got, counted) = allocations(|| {
        within(
            "соседний поток аллоцирует 1000 раз",
            || {
                let boxes: Vec<Box<u64>> = (0..1000).map(Box::new).collect();
                boxes.len()
            },
        )
    });
    assert_eq!(got, 1000);
    assert!(
        counted < 100,
        "allocations насчитал {counted}: он видит чужие потоки. Счётчик должен жить в thread_local, иначе соседние тесты портят друг другу числа"
    );
}

fn micros(range: std::ops::RangeInclusive<u64>) -> Vec<Duration> {
    range.map(Duration::from_micros).collect()
}

#[test]
fn summary_uses_nearest_rank() {
    let mut samples = micros(1..=100);
    samples.reverse();
    let summary = Summary::from_samples(samples).expect("сто замеров дают сводку");
    assert_eq!(summary.runs(), 100);
    assert_eq!(
        summary.median(),
        Duration::from_micros(50),
        "медиана ста замеров 1..=100 мкс"
    );
    assert_eq!(
        summary.p99(),
        Duration::from_micros(99),
        "p99 ста замеров 1..=100 мкс"
    );
    assert_eq!(summary.percentile(100), Duration::from_micros(100));
    assert_eq!(summary.percentile(1), Duration::from_micros(1));
    assert_eq!(
        summary.to_string(),
        "медиана 50.0 мкс, p99 99.0 мкс, замеров 100"
    );
}

#[test]
fn summary_of_odd_and_tiny_samples() {
    let odd = Summary::from_samples(micros(1..=101)).expect("сводка есть");
    assert_eq!(
        odd.median(),
        Duration::from_micros(51),
        "у 101 замера медиана это 51-й"
    );
    assert_eq!(odd.p99(), Duration::from_micros(100));
    let one = Summary::from_samples(micros(7..=7)).expect("сводка есть");
    assert_eq!(
        (one.median(), one.p99()),
        (Duration::from_micros(7), Duration::from_micros(7)),
        "у одного замера все процентили равны ему"
    );
    assert!(
        Summary::from_samples(Vec::new()).is_none(),
        "без замеров сводки нет, а не паника и не ноль"
    );
}

#[test]
fn measure_warms_up_then_measures() {
    let mut calls = 0;
    let summary = measure(3, 10, || {
        calls += 1;
        calls
    })
    .expect("десять замеров дают сводку");
    assert_eq!(
        calls, 13,
        "measure(3, 10, f) зовёт f три раза на прогрев и десять на замер"
    );
    assert_eq!(summary.runs(), 10, "прогрев в замеры не попадает");
    assert!(summary.median() <= summary.p99());
    assert!(measure(5, 0, || ()).is_none(), "ноль замеров это None");
}

#[test]
fn mem_network_still_works_next_to_fakes() {
    let mut net = MemNetwork::new().with_clock(FakeClock::new(10));
    let alice = net.register("alice").expect("alice регистрируется");
    let post = net
        .publish(alice, "время из часов", Visibility::Public)
        .expect("пост публикуется");
    assert_eq!(net.post(post).map(Post::time), Some(10));
}
