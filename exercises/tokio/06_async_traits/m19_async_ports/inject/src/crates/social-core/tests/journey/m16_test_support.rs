use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use social_core::{
    App, Clock, MemNetwork, MemUserStore, Network, Notification, Notifiers, Post, PostId,
    PostStore, SystemClock, TRENDS_WINDOW, UserId, UserStore, Visibility, execute_line,
};
use test_support::{
    CountingAlloc, FakeClock, FakePosts, FakeUsers, RecordingNotifier, allocations,
};
use tokio::runtime::{Builder, Runtime};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

const START: u64 = 1_700_000_000;

const DEADLINE: Duration = Duration::from_secs(10);

const PAGE_BUDGET: usize = 10;

struct Live {
    runtime: Runtime,
    app: App,
}

impl Live {
    fn start<U, P>(network: Network<U, P>, notifiers: Notifiers) -> Live
    where
        U: UserStore + Send + 'static,
        P: PostStore + Send + 'static,
    {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("рантайм теста строится");
        let app = {
            let _entered = runtime.enter();
            App::with_notifiers(network, notifiers).expect("ядро запускается внутри рантайма")
        };
        Live { runtime, app }
    }

    fn run(&self, line: &str) -> String {
        self.runtime
            .block_on(execute_line(line, &self.app))
            .unwrap_or_else(|e| panic!("`{line}` должна пройти, а вышло: {e}"))
    }
}

type Shared = Live;

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

fn register(net: &mut MemNetwork, name: &str) -> UserId {
    net.register(name)
        .unwrap_or_else(|e| panic!("{name} должен регистрироваться: {e}"))
}

fn publish(net: &mut MemNetwork, author: UserId, text: &str) -> PostId {
    net.publish(author, text, Visibility::Public)
        .unwrap_or_else(|e| panic!("пост «{text}» должен публиковаться: {e}"))
}

#[test]
fn network_takes_time_from_its_clock() {
    let clock = FakeClock::new(START);
    let mut net = MemNetwork::new().with_clock(clock.clone());
    let alice = register(&mut net, "alice");
    let first = publish(&mut net, alice, "первый");
    clock.advance(60);
    let second = publish(&mut net, alice, "второй");
    assert_eq!(net.now(), START + 60, "Network::now() спрашивает свои часы");
    assert_eq!(
        net.post(first).map(Post::time),
        Some(START),
        "publish берёт время у часов сети, а не у SystemTime"
    );
    assert_eq!(net.post(second).map(Post::time), Some(START + 60));
}

#[test]
fn default_clock_is_the_system_clock() {
    let wall = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_secs())
            .expect("системные часы после 1970 года")
    };
    let before = wall();
    let from_clock = SystemClock.now();
    let mut net = MemNetwork::new();
    let alice = register(&mut net, "alice");
    let post = publish(&mut net, alice, "по настоящим часам");
    let after = wall();
    assert!(
        (before..=after).contains(&from_clock),
        "SystemClock::now() это секунды с 1970 года по системным часам"
    );
    let time = net.post(post).map(Post::time).unwrap_or_default();
    assert!(
        (before..=after).contains(&time),
        "без with_clock сеть живёт по SystemClock: время поста {time} вне [{before}, {after}]"
    );
}

#[test]
fn author_page_orders_posts_by_clock_time() {
    let clock = FakeClock::new(START);
    let mut net = MemNetwork::new().with_clock(clock.clone());
    let alice = register(&mut net, "alice");
    let bob = register(&mut net, "bob");
    clock.set(START + 300);
    let late = publish(&mut net, alice, "поздний");
    clock.set(START + 100);
    let early = publish(&mut net, alice, "ранний");
    clock.set(START + 200);
    let middle = publish(&mut net, alice, "средний");
    let page = net
        .author_page(bob, alice, None, 10)
        .expect("страница автора строится");
    let ids: Vec<PostId> = page.posts.iter().map(|post| post.id()).collect();
    assert_eq!(
        ids,
        [late, middle, early],
        "страница автора идёт от новых к старым по времени часов, а не по порядку публикации"
    );
}

#[test]
fn trends_command_uses_the_network_clock() {
    let clock = FakeClock::new(START);
    let mut net = MemNetwork::new().with_clock(clock.clone());
    let alice = register(&mut net, "alice");
    let bob = register(&mut net, "bob");
    let post = publish(&mut net, alice, "в тренды");
    net.like(bob, post).expect("лайк проходит");
    let (fresh, stale) = within("тренды по часам сети", move || {
        let net = Live::start(net, Notifiers::new());
        let fresh = net.run("trends");
        clock.advance(TRENDS_WINDOW);
        (fresh, net.run("trends"))
    });
    assert_eq!(
        fresh, "@alice [1] в тренды (лайков: 1), за сутки +1",
        "команда trends считает окно от часов сети: лайк поставлен по FakeClock, и сейчас по нему же"
    );
    assert_eq!(
        stale, "трендов пока нет",
        "часы сети ушли на сутки вперёд, лайк выпал из окна"
    );
}

#[test]
fn fake_stores_shift_ids_through_the_commands() {
    let network = Network::with_stores(FakeUsers::starting_at(1000), FakePosts::starting_at(7000))
        .with_clock(FakeClock::new(START));
    let out = within(
        "команды на фейковых хранилищах",
        move || {
            let net = Live::start(network, Notifiers::new());
            [
                "register alice",
                "register bob",
                "follow bob alice",
                "post alice сдвинутые id",
                "like bob 7000",
                "feed bob",
            ]
            .map(|line| net.run(line))
        },
    );
    assert_eq!(out[0], "зарегистрирован @alice (id 1000)");
    assert_eq!(out[1], "зарегистрирован @bob (id 1001)");
    assert_eq!(
        out[3], "опубликован пост [7000]",
        "id поста выдаёт хранилище"
    );
    assert_eq!(out[4], "@bob лайкнул пост [7000]");
    assert_eq!(out[5], "@alice [7000] сдвинутые id (лайков: 1)");
}

#[test]
fn index_paths_never_scan_the_whole_store() {
    let posts = FakePosts::starting_at(1);
    let scans = posts.scans();
    let mut net =
        Network::with_stores(MemUserStore::new(), posts).with_clock(FakeClock::new(START));
    let alice = net.register("alice").expect("alice регистрируется");
    let bob = net.register("bob").expect("bob регистрируется");
    for n in 0..30 {
        net.publish(alice, &format!("пост {n} #rust"), Visibility::Public)
            .expect("пост публикуется");
    }
    let before = scans.get();
    let page = net
        .author_page(bob, alice, None, 5)
        .expect("страница автора строится");
    assert_eq!(page.posts.len(), 5);
    assert_eq!(net.search_tag(bob, "rust").count(), 30);
    assert_eq!(
        scans.get(),
        before,
        "страница автора и поиск по тегу идут по индексам и не обходят все посты хранилища"
    );
    assert_eq!(net.feed(alice).take(3).count(), 3);
    assert_eq!(
        scans.get(),
        before + 1,
        "одна лента это один проход по хранилищу"
    );
}

#[test]
fn recording_notifier_sees_every_notification() {
    let log = Arc::new(RecordingNotifier::new());
    let mut notifiers = Notifiers::new();
    notifiers.add(log.clone());
    let mut network = MemNetwork::new().with_clock(FakeClock::new(START));
    let alice = register(&mut network, "alice");
    let bob = register(&mut network, "bob");
    let post = within(
        "подписка, пост, лайк и сообщение",
        move || {
            let live = Live::start(network, notifiers);
            let net = &live.app;
            live.runtime.block_on(async {
                net.follow(bob, alice).await.expect("подписка проходит");
                let post = net
                    .publish(alice, "для подписчиков", Visibility::Public)
                    .await
                    .expect("пост публикуется");
                net.like(bob, post).await.expect("лайк проходит");
                net.send_message(bob, alice, "привет")
                    .await
                    .expect("сообщение уходит");
                net.flush().await.expect("рассылка доходит вовремя");
                post
            })
        },
    );
    let calls = log.calls();
    let expected = [
        (alice, Notification::Followed { by: bob }),
        (
            bob,
            Notification::NewPost {
                author: alice,
                post,
            },
        ),
        (alice, Notification::Liked { by: bob, post }),
        (alice, Notification::Message { from: bob }),
    ];
    assert_eq!(
        calls.len(),
        expected.len(),
        "уведомитель из реестра получает каждое уведомление ровно один раз, а журнал такой: {calls:?}"
    );
    for call in expected {
        assert!(calls.contains(&call), "в журнале нет {call:?}: {calls:?}");
    }
    assert_eq!(
        log.for_user(bob),
        [Notification::NewPost {
            author: alice,
            post
        }]
    );
}

fn busy_network() -> Shared {
    let mut net = MemNetwork::new().with_clock(FakeClock::new(START));
    let bob = register(&mut net, "bob");
    for n in 0..10 {
        let author = register(&mut net, &format!("author{n}"));
        net.follow(bob, author).expect("подписка проходит");
        for k in 0..12 {
            publish(&mut net, author, &format!("пост {k} от author{n} #m16"));
        }
    }
    Live::start(net, Notifiers::new())
}

fn page_allocations(command: &'static str) -> Vec<(String, usize, usize)> {
    within(command, move || {
        let net = busy_network();
        net.run(&format!("{command} 5"));
        [5, 20, 50]
            .into_iter()
            .map(|size| {
                let line = format!("{command} {size}");
                let (out, count) =
                    allocations(|| net.runtime.block_on(execute_line(&line, &net.app)));
                let out = out.unwrap_or_else(|e| panic!("`{line}` должна пройти, а вышло: {e}"));
                assert_eq!(
                    out.lines().count(),
                    size,
                    "`{line}` показывает {size} постов"
                );
                (line, size, count)
            })
            .collect()
    })
}

fn assert_budget(measured: Vec<(String, usize, usize)>) {
    let smallest = measured.first().map_or(0, |(_, _, count)| *count);
    for (line, size, count) in &measured {
        assert!(
            *count <= smallest + 1,
            "`{line}`: аллокаций на страницу из {size} постов {count}, а на страницу из 5 было \
             {smallest}. Число аллокаций не должно расти с размером страницы: каждый вызов порта \
             может стоить коробку под future, а пост не должен стоить ни одной (Arc<str> вместо \
             копий строк, Vec с запасом)"
        );
    }
    for (line, size, count) in measured {
        assert!(
            count <= PAGE_BUDGET,
            "`{line}`: аллокаций на страницу из {size} постов {count}, а можно не больше \
             {PAGE_BUDGET}, сколько бы постов ни было на странице. Ответ пишется в одну строку \
             с запасом (String::with_capacity и write!), ник автора выводится через Display \
             без своей String, а страница из pages() сразу резервирует место"
        );
    }
}

#[test]
fn feed_command_allocations_do_not_grow_with_the_page() {
    assert_budget(page_allocations("feed bob"));
}

#[test]
fn page_command_allocations_do_not_grow_with_the_page() {
    assert_budget(page_allocations("page bob 2"));
}

fn manifest(path: &Path) -> String {
    fs::read_to_string(path.join("Cargo.toml"))
        .unwrap_or_else(|e| panic!("{} должен читаться: {e}", path.join("Cargo.toml").display()))
}

fn section<'a>(manifest: &'a str, name: &str) -> Vec<&'a str> {
    let header = format!("[{name}]");
    let mut inside = false;
    let mut lines = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == header;
        } else if inside && !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

fn crate_dir(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn test_support_is_a_dev_only_dependency() {
    let core = manifest(&crate_dir(""));
    assert!(
        !section(&core, "dependencies")
            .iter()
            .any(|line| line.starts_with("test-support")),
        "test-support попал в [dependencies] social-core: фейки и счётчик аллокаций уедут в релиз. Его место в [dev-dependencies]"
    );
    assert!(
        section(&core, "dev-dependencies")
            .iter()
            .any(|line| line.starts_with("test-support") && line.contains("test-util")),
        "в [dev-dependencies] social-core нужен test-support с feature test-util: \
         test-support = {{ workspace = true, features = [\"test-util\"] }}"
    );
    let support = manifest(&crate_dir("../test-support"));
    assert!(
        section(&support, "features")
            .iter()
            .any(|line| line.starts_with("test-util")),
        "у test-support должна быть feature test-util, за которой живут фейки"
    );
    let repl = manifest(&crate_dir("../../../tools/repl"));
    assert!(
        !repl.contains("test-support"),
        "терминал это релизный бинарь: test-support ему не нужен даже в dev-dependencies"
    );
}
