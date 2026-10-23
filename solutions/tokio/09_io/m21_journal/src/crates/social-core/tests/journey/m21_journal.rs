use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use contracts::{CommentId, Entry, write_line};
use social_core::{
    App, AppConfig, AppError, BoxFuture, DomainError, FollowCounts, Journal, JournalError,
    JournalWriter, MemNetwork, MemRepo, Notifiers, Op, PostId, Repos, User, UserId, UserRepo,
    Visibility, replay,
};
use test_support::{Chunked, FakeClock, Probe, RecordingNotifier, Trickle};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(10);
const CLOSE: Duration = Duration::from_secs(5);
const MAX_LINE: usize = 1024;

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn registered(id: u64, name: &str) -> Entry {
    Entry::Registered {
        user: UserId(id),
        name: name.to_string(),
    }
}

fn sample() -> Vec<Entry> {
    vec![
        registered(1, "alice"),
        registered(2, "bob"),
        Entry::Followed {
            who: UserId(2),
            whom: UserId(1),
        },
        Entry::Posted {
            post: PostId(1),
            author: UserId(1),
            text: "две\nстроки\tи табуляция, «ёлочки» и \"кавычки\"".to_string(),
            visibility: Visibility::Public,
            at: 1_000,
        },
        Entry::Liked {
            post: PostId(1),
            by: UserId(2),
            at: 1_005,
        },
    ]
}

fn encoded(entries: &[Entry]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in entries {
        write_line(&mut bytes, entry).expect("запись в Vec не падает");
    }
    bytes
}

async fn replayed(bytes: &[u8]) -> Vec<Entry> {
    within("проигрывание журнала", replay(bytes, MAX_LINE))
        .await
        .unwrap_or_else(|e| panic!("журнал, который написало ядро, должен проигрываться: {e}"))
        .entries
}

fn journaled(clock: &FakeClock, probe: &Probe, notifiers: Notifiers) -> App {
    let network = MemNetwork::new().with_clock(clock.clone());
    let config = AppConfig::default();
    let journal = Journal::new(probe.clone());
    App::journaled(MemRepo::repos(network), notifiers, config, journal)
        .expect("ядро с журналом запускается внутри рантайма")
}

async fn ok<T>(what: &str, work: impl Future<Output = Result<T, AppError>>) -> T {
    within(what, work)
        .await
        .unwrap_or_else(|e| panic!("{what}: ждали успех, а пришло {e:?}"))
}

struct Story {
    alice: UserId,
    bob: UserId,
    first: PostId,
    second: PostId,
    comment: CommentId,
    answer: CommentId,
}

async fn story(app: &App, clock: &FakeClock) -> Story {
    let alice = ok("регистрация alice", app.register("alice")).await;
    let bob = ok("регистрация bob", app.register("bob")).await;
    assert!(
        app.register("alice").await.is_err(),
        "занятый ник по-прежнему отказ"
    );
    ok("подписка", app.follow(bob, alice)).await;
    ok("повторная подписка", app.follow(bob, alice)).await;
    let first = ok(
        "первый пост",
        app.publish(alice, "Журнал помнит всё #rust", Visibility::Public),
    )
    .await;
    clock.advance(5);
    ok("лайк", app.like(bob, first)).await;
    ok("повторный лайк", app.like(bob, first)).await;
    let comment = ok("комментарий", app.comment(bob, first, "Наконец-то")).await;
    let answer = ok("ответ", app.reply(alice, comment, "Ага")).await;
    ok("сообщение", app.send_message(bob, alice, "привет")).await;
    let second = ok(
        "второй пост",
        app.publish(alice, "Только для своих", Visibility::Followers),
    )
    .await;
    ok("скрыть автора", app.mute(alice, bob)).await;
    ok("удаление", app.delete_post(alice, second)).await;
    Story {
        alice,
        bob,
        first,
        second,
        comment,
        answer,
    }
}

#[tokio::test]
async fn writer_flushes_every_n_entries_and_on_close() {
    let probe = Probe::new();
    let mut writer = JournalWriter::new(probe.clone(), 3);
    for (id, name) in [(1, "alice"), (2, "bob")] {
        within("запись", writer.append(&registered(id, name)))
            .await
            .expect("запись в пробник не падает");
    }
    assert_eq!(
        probe.lines(),
        0,
        "две записи из трёх лежат в BufWriter: до порога сброса приёмник ничего не видит"
    );
    within("запись", writer.append(&registered(3, "carol")))
        .await
        .expect("запись в пробник не падает");
    assert_eq!(
        probe.lines(),
        3,
        "на третьей записи (flush_every = 3) буфер сбрасывается целиком"
    );
    within("запись", writer.append(&registered(4, "dave")))
        .await
        .expect("запись в пробник не падает");
    assert_eq!(
        probe.lines(),
        3,
        "после сброса счёт начинается заново: четвёртая запись ждёт в буфере"
    );
    within("закрытие", writer.close())
        .await
        .expect("закрытие пробника не падает");
    assert_eq!(probe.lines(), 4, "close сбрасывает остаток");
    assert!(
        probe.is_shut(),
        "close закрывает запись у приёмника (shutdown): асинхронного Drop нет"
    );
}

#[tokio::test]
async fn lines_survive_a_chunked_sink_and_a_trickling_source() {
    let probe = Probe::new();
    let mut writer = JournalWriter::new(Chunked::new(probe.clone(), 5), 2);
    for entry in &sample() {
        within("запись", writer.append(entry))
            .await
            .expect("запись в пробник не падает");
    }
    within("закрытие", writer.close())
        .await
        .expect("закрытие пробника не падает");
    assert!(
        probe.writes() > sample().len(),
        "приёмник берёт по 5 байт: строку надо дописывать до конца (write_all), а не одним write"
    );
    let bytes = probe.bytes();
    assert_eq!(
        bytes,
        encoded(&sample()),
        "каждая запись это одна строка JSON с \\n в конце, перевод строки внутри текста экранирован"
    );
    let replayed = within(
        "проигрывание",
        replay(Trickle::new(&bytes[..], 3), MAX_LINE),
    )
    .await
    .expect("журнал, отданный по три байта, проигрывается");
    assert_eq!(
        replayed.entries,
        sample(),
        "источник отдаёт по три байта с паузами: события возвращаются те же и в том же порядке"
    );
    assert!(!replayed.torn, "все строки целые, хвоста нет");
    assert_eq!(
        replayed.bytes,
        bytes.len() as u64,
        "bytes считает все целые строки"
    );
}

#[tokio::test]
async fn torn_tail_is_dropped_and_measured() {
    let whole = encoded(&sample()[..2]);
    let mut bytes = whole.clone();
    bytes.extend_from_slice(br#"{"event":"followed","who":"u_2""#);
    let replayed = within("проигрывание", replay(&bytes[..], MAX_LINE))
        .await
        .expect("недописанная последняя строка это след обрыва, а не ошибка");
    assert_eq!(
        replayed.entries,
        sample()[..2],
        "недописанная строка в состояние не попадает"
    );
    assert!(replayed.torn, "Replayed::torn сообщает, что хвост был");
    assert_eq!(
        replayed.bytes,
        whole.len() as u64,
        "bytes считает только целые строки: до этой длины журнал потом обрезается"
    );
}

#[tokio::test]
async fn endless_line_is_refused() {
    let head = encoded(&sample()[..1]);
    let source = (&head[..]).chain(tokio::io::repeat(b'x').take(8 << 20));
    let error = within("строка на 8 МиБ", replay(source, 256))
        .await
        .expect_err("строка длиннее max_line это ошибка, а не повод читать дальше");
    assert!(
        matches!(error, JournalError::TooLong { line: 2, max: 256 }),
        "ждали TooLong {{ line: 2, max: 256 }}, а пришло {error:?}"
    );
}

#[tokio::test]
async fn broken_line_is_an_error_with_its_number() {
    let mut bytes = encoded(&sample()[..1]);
    bytes.extend_from_slice(b"{\"event\":\"exploded\"}\n");
    bytes.extend(encoded(&sample()[1..2]));
    let error = within("проигрывание", replay(&bytes[..], MAX_LINE))
        .await
        .expect_err("целая, но испорченная строка значит, что журналу нельзя верить");
    assert!(
        matches!(error, JournalError::Bad { line: 2, .. }),
        "ждали Bad {{ line: 2, .. }}, а пришло {error:?}"
    );
    assert_eq!(error.line(), Some(2), "line() называет номер строки");
}

#[tokio::test]
async fn app_writes_every_change_to_the_journal() {
    let clock = FakeClock::new(1_000);
    let probe = Probe::new();
    let app = journaled(&clock, &probe, Notifiers::new());
    let s = story(&app, &clock).await;
    let report = within("остановка", app.close(CLOSE)).await;
    assert_eq!(
        report.order(),
        ["sources", "handlers", "trends", "journal"],
        "журнал это последняя стадия остановки: всё, что успели сделать остальные, должно в него попасть"
    );
    assert!(
        probe.is_shut(),
        "после close журнал сброшен и закрыт (shutdown)"
    );
    let text = |s: &str| s.to_string();
    assert_eq!(
        replayed(&probe.bytes()).await,
        vec![
            registered(1, "alice"),
            registered(2, "bob"),
            Entry::Followed {
                who: s.bob,
                whom: s.alice,
            },
            Entry::Posted {
                post: s.first,
                author: s.alice,
                text: text("Журнал помнит всё #rust"),
                visibility: Visibility::Public,
                at: 1_000,
            },
            Entry::Liked {
                post: s.first,
                by: s.bob,
                at: 1_005,
            },
            Entry::Commented {
                comment: s.comment,
                post: s.first,
                author: s.bob,
                text: text("Наконец-то"),
            },
            Entry::Replied {
                comment: s.answer,
                parent: s.comment,
                author: s.alice,
                text: text("Ага"),
            },
            Entry::MessageSent {
                from: s.bob,
                to: s.alice,
                text: text("привет"),
            },
            Entry::Posted {
                post: s.second,
                author: s.alice,
                text: text("Только для своих"),
                visibility: Visibility::Followers,
                at: 1_005,
            },
            Entry::Muted {
                who: s.alice,
                whom: s.bob,
            },
            Entry::Deleted {
                post: s.second,
                by: s.alice,
            },
        ],
        "в журнале каждое настоящее изменение по порядку: отказы и повторы (вторая подписка, второй лайк, занятый ник) не пишутся"
    );
    assert_eq!(
        app.register("carol").await,
        Err(AppError::Stopped(Op::Journal)),
        "после close журнал закрыт: запись отвечает Stopped(Op::Journal), а не теряется молча"
    );
}

struct Jittery {
    inner: Arc<dyn UserRepo>,
}

impl UserRepo for Jittery {
    fn register(&self, name: String) -> BoxFuture<'_, Result<UserId, DomainError>> {
        Box::pin(async move {
            let id = self.inner.register(name).await?;
            tokio::time::sleep(Duration::from_millis(id.0 % 4)).await;
            Ok(id)
        })
    }

    fn user(&self, id: UserId) -> BoxFuture<'_, Option<User>> {
        self.inner.user(id)
    }

    fn find(&self, name: String) -> BoxFuture<'_, Option<User>> {
        self.inner.find(name)
    }

    fn follow(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, DomainError>> {
        self.inner.follow(who, whom)
    }

    fn mute(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, DomainError>> {
        self.inner.mute(who, whom)
    }

    fn is_following(&self, who: UserId, whom: UserId) -> BoxFuture<'_, bool> {
        self.inner.is_following(who, whom)
    }

    fn follow_counts(&self, id: UserId) -> BoxFuture<'_, FollowCounts> {
        self.inner.follow_counts(id)
    }

    fn count(&self) -> BoxFuture<'_, usize> {
        self.inner.count()
    }
}

#[tokio::test]
async fn concurrent_writes_land_in_the_order_of_their_ids() {
    let probe = Probe::new();
    let mem = MemRepo::repos(MemNetwork::new());
    let repos = Repos {
        users: Arc::new(Jittery { inner: mem.users }),
        posts: mem.posts,
        chats: mem.chats,
    };
    let journal = Journal::new(probe.clone());
    let app = App::journaled(repos, Notifiers::new(), AppConfig::default(), journal)
        .expect("ядро с журналом запускается внутри рантайма");
    let mut tasks = tokio::task::JoinSet::new();
    for n in 0..32 {
        let app = app.clone();
        tasks.spawn(async move { app.register(&format!("user_{n}")).await });
    }
    while let Some(done) = within("регистрации", tasks.join_next()).await {
        done.expect("задача регистрации не падает")
            .expect("ники разные, регистрация проходит");
    }
    within("остановка", app.close(CLOSE)).await;
    let entries = replayed(&probe.bytes()).await;
    assert_eq!(entries.len(), 32, "в журнале все 32 регистрации");
    let mut network = MemNetwork::new();
    network.restore(&entries).unwrap_or_else(|e| {
        panic!(
            "32 одновременные регистрации, хранилище отвечает с разной задержкой: \
             журнал должен лечь в том порядке, в каком ядро выдало id ({e})"
        )
    });
}

#[tokio::test]
async fn restored_network_is_the_same() {
    let clock = FakeClock::new(1_000);
    let probe = Probe::new();
    let app = journaled(&clock, &probe, Notifiers::new());
    let s = story(&app, &clock).await;
    within("остановка", app.close(CLOSE)).await;
    let entries = replayed(&probe.bytes()).await;

    let mut network = MemNetwork::new().with_clock(FakeClock::new(9_000));
    let applied = network
        .restore(&entries)
        .unwrap_or_else(|e| panic!("журнал своего же ядра должен проигрываться: {e}"));
    assert_eq!(
        applied,
        entries.len(),
        "restore отдаёт число применённых записей"
    );
    let notes = Arc::new(RecordingNotifier::new());
    let mut notifiers = Notifiers::new();
    notifiers.add(notes.clone());
    let app = App::with_notifiers(network, notifiers).expect("ядро запускается");

    let alice = app.find_user("alice").await.expect("alice вернулась");
    assert_eq!(
        alice.id(),
        s.alice,
        "id пользователей те же, что до перезапуска"
    );
    assert!(
        app.is_following(s.bob, s.alice).await,
        "подписка восстановлена"
    );
    let feed = app.feed(s.bob, 0, 10).await;
    assert_eq!(
        feed.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![s.first],
        "в ленте bob только первый пост: второй удалён, и удаление тоже в журнале"
    );
    let first = app.post(s.first).await.expect("первый пост на месте");
    assert_eq!(
        &*first.text, "Журнал помнит всё #rust",
        "текст поста восстановлен"
    );
    assert_eq!(
        first.time, 1_000,
        "время поста берётся из журнала, а не из часов при запуске"
    );
    assert_eq!(first.likes, 1, "лайк восстановлен");
    let thread = app
        .thread(s.bob, s.first)
        .await
        .expect("ветка восстановленного поста читается");
    assert_eq!(
        thread
            .iter()
            .map(|(depth, comment)| (*depth, comment.id()))
            .collect::<Vec<_>>(),
        vec![(0, s.comment), (1, s.answer)],
        "комментарий и ответ на него восстановлены с теми же id"
    );
    let history = app
        .history(s.bob, s.alice, 10)
        .await
        .expect("история переписки читается");
    assert_eq!(
        history.iter().map(|m| m.text()).collect::<Vec<_>>(),
        ["привет"],
        "личное сообщение восстановлено"
    );
    ok("доставка", app.flush()).await;
    assert_eq!(
        notes.count(),
        0,
        "проигрывание журнала никого не уведомляет заново: оно идёт мимо App"
    );
    assert_eq!(
        ok("новый пользователь", app.register("carol")).await,
        UserId(3),
        "после восстановления id продолжаются с того места, где остановились"
    );
    within("остановка", app.close(CLOSE)).await;
}

#[tokio::test]
async fn restore_refuses_a_journal_that_does_not_fit() {
    let mut network = MemNetwork::new();
    let error = network
        .restore(&[registered(2, "alice")])
        .expect_err("в журнале u_2, а ядро выдало u_1: так журнал не проигрывается");
    assert!(
        matches!(error, JournalError::Diverged { line: 1, .. }),
        "ждали Diverged {{ line: 1, .. }}, а пришло {error:?}"
    );
    let mut network = MemNetwork::new();
    let error = network
        .restore(&[
            registered(1, "alice"),
            Entry::Followed {
                who: UserId(1),
                whom: UserId(7),
            },
        ])
        .expect_err("подписка на несуществующего пользователя не применяется");
    assert!(
        matches!(error, JournalError::Rejected { line: 2, .. }),
        "ждали Rejected {{ line: 2, .. }}, а пришло {error:?}"
    );
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m21")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

async fn reopen(path: &PathBuf) -> App {
    within(
        "запуск из журнала",
        App::restore(
            MemNetwork::new(),
            Notifiers::new(),
            AppConfig::default(),
            path,
        ),
    )
    .await
    .unwrap_or_else(|e| panic!("ядро поднимается из {}: {e}", path.display()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn journal_file_survives_restarts() {
    let path = scratch("restart").join("data").join("journal.ndjson");
    let first = reopen(&path).await;
    let alice = ok("регистрация alice", first.register("alice")).await;
    let bob = ok("регистрация bob", first.register("bob")).await;
    ok("подписка", first.follow(bob, alice)).await;
    let post = ok(
        "пост",
        first.publish(alice, "Переживу перезапуск", Visibility::Public),
    )
    .await;
    within("остановка", first.close(CLOSE)).await;

    let second = reopen(&path).await;
    let feed = second.feed(bob, 0, 10).await;
    assert_eq!(
        feed.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![post],
        "после перезапуска лента bob та же: ядро проиграло журнал из файла"
    );
    ok("регистрация carol", second.register("carol")).await;
    within("остановка", second.close(CLOSE)).await;

    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .await
        .expect("журнал открывается на дозапись");
    file.write_all(br#"{"event":"registered","user":"u_4","na"#)
        .await
        .expect("обрывок дописывается");
    file.shutdown().await.expect("файл закрывается");
    drop(file);

    let third = reopen(&path).await;
    assert_eq!(
        third.user_count().await,
        3,
        "обрывок последней строки не попадает в состояние"
    );
    ok("регистрация dave", third.register("dave")).await;
    within("остановка", third.close(CLOSE)).await;

    let fourth = reopen(&path).await;
    assert!(
        fourth.find_user("dave").await.is_some(),
        "запуск отрезал обрывок, поэтому новая запись легла с новой строки и читается"
    );
    assert_eq!(fourth.user_count().await, 4, "в журнале все четверо");
    within("остановка", fourth.close(CLOSE)).await;
}
