use std::future::Future;
use std::time::Duration;

use futures::{Stream, StreamExt, TryStreamExt};
use social_core::{
    App, AppConfig, AppError, DomainError, FeedEvent, MemNetwork, MemRepo, Notifiers, PostId,
    PostItem, Repos, UserId, Visibility,
};
use test_support::SlowRepos;
use tokio::time::{Instant, timeout};

const DEADLINE: Duration = Duration::from_secs(10);
const PACE: Duration = Duration::from_millis(100);

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn start(config: AppConfig) -> App {
    App::start(MemNetwork::new(), Notifiers::new(), config).expect("ядро запускается в рантайме")
}

fn over(repos: Repos, config: AppConfig) -> App {
    App::with_repos(repos, Notifiers::new(), config).expect("ядро запускается в рантайме")
}

async fn user(app: &App, name: &str) -> UserId {
    within("регистрация", app.register(name))
        .await
        .unwrap_or_else(|e| panic!("{name} регистрируется: {e:?}"))
}

async fn follow(app: &App, who: UserId, whom: UserId) {
    within("подписка", app.follow(who, whom))
        .await
        .expect("подписка проходит");
}

async fn post(app: &App, author: UserId, text: &str) -> PostId {
    within("публикация", app.publish(author, text, Visibility::Public))
        .await
        .expect("пост публикуется")
}

fn ids(items: &[PostItem]) -> Vec<PostId> {
    items.iter().map(|item| item.id).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    History(PostId),
    UpToDate,
    Live(PostId),
}

fn seen(event: &FeedEvent) -> Seen {
    match event {
        FeedEvent::History(post) => Seen::History(post.id),
        FeedEvent::UpToDate => Seen::UpToDate,
        FeedEvent::Live(post) => Seen::Live(post.id),
    }
}

async fn next<S>(watch: &mut S) -> FeedEvent
where
    S: Stream<Item = Result<FeedEvent, AppError>> + Unpin,
{
    within("следующее событие ленты", watch.next())
        .await
        .expect("живая лента не кончается, пока ядро работает")
        .unwrap_or_else(|e| panic!("событие ленты пришло с ошибкой: {e:?}"))
}

async fn until_quiet<S>(watch: &mut S, quiet: Duration) -> Vec<Seen>
where
    S: Stream<Item = Result<FeedEvent, AppError>> + Unpin,
{
    let mut events = Vec::new();
    while let Ok(event) = timeout(quiet, watch.next()).await {
        let event = event
            .expect("живая лента не кончается, пока ядро работает")
            .unwrap_or_else(|e| panic!("событие ленты пришло с ошибкой: {e:?}"));
        events.push(seen(&event));
    }
    events
}

fn assert_stream<S: Stream + Send + 'static>(_: &S) {}

#[tokio::test]
async fn feed_stream_walks_pages_by_cursor() {
    let app = start(AppConfig {
        feed_page: 4,
        ..AppConfig::default()
    });
    let alice = user(&app, "alice").await;
    let bob = user(&app, "bob").await;
    let carol = user(&app, "carol").await;
    follow(&app, bob, alice).await;
    let mut expected = Vec::new();
    for n in 1..=10 {
        expected.push(post(&app, alice, &format!("пост {n}")).await);
    }
    post(&app, carol, "чужой пост").await;
    expected.push(post(&app, bob, "свой пост").await);
    expected.reverse();

    let mut stream = Box::pin(app.feed_stream(bob));
    assert_stream(&stream);
    let mut items = Vec::new();
    for _ in 0..6 {
        let item = within("пост ленты", stream.try_next())
            .await
            .unwrap_or_else(|e| panic!("страница ленты читается: {e:?}"))
            .expect("в ленте bob одиннадцать постов, а стрим кончился раньше");
        items.push(item);
    }
    for n in 1..=3 {
        post(&app, alice, &format!("поздний пост {n}")).await;
    }
    let rest: Vec<PostItem> = within("остаток ленты", stream.try_collect())
        .await
        .unwrap_or_else(|e| panic!("страница ленты читается: {e:?}"));
    items.extend(rest);
    assert_eq!(
        ids(&items),
        expected,
        "лента идёт от новых к старым страницами по курсору: посты, вышедшие посреди чтения, не сдвигают страницы, поэтому нет ни повторов, ни пропусков, а чужих постов нет вовсе"
    );
    let names: Vec<&str> = items.iter().map(|item| &*item.author_name).collect();
    assert_eq!(names[0], "bob", "имя автора подтянуто к каждому посту");
    assert!(
        names[1..].iter().all(|name| *name == "alice"),
        "имя автора подтянуто к каждому посту, а вышло {names:?}"
    );
}

#[tokio::test]
async fn unknown_viewer_is_an_error_in_the_stream() {
    let app = start(AppConfig::default());
    let mut stream = Box::pin(app.feed_stream(UserId(99)));
    let first = within("первая страница", stream.try_next()).await;
    assert!(
        matches!(
            first,
            Err(AppError::Domain(DomainError::UnknownUser(UserId(99))))
        ),
        "ошибка страницы идёт по стриму как Err, а не теряется и не паникует; пришло {first:?}"
    );
}

async fn readers_world(authors: usize, posts_each: usize) -> (Repos, UserId) {
    let repos = MemRepo::repos(MemNetwork::new());
    let setup = over(repos.clone(), AppConfig::default());
    let reader = user(&setup, "reader").await;
    for n in 0..authors {
        let author = user(&setup, &format!("author_{n}")).await;
        follow(&setup, reader, author).await;
        for k in 0..posts_each {
            post(&setup, author, &format!("пост {k} автора {n}")).await;
        }
    }
    within("остановка", setup.close(DEADLINE)).await;
    (repos, reader)
}

#[tokio::test(start_paused = true)]
async fn authors_are_looked_up_in_parallel_with_a_limit() {
    let (repos, reader) = readers_world(8, 1).await;
    let slow = SlowRepos::wrap(repos, PACE);
    let app = over(
        slow.repos(),
        AppConfig {
            feed_page: 20,
            feed_lookups: 4,
            ..AppConfig::default()
        },
    );
    let started = Instant::now();
    let items: Vec<PostItem> = within("лента", app.feed_stream(reader).try_collect())
        .await
        .unwrap_or_else(|e| panic!("лента читается: {e:?}"));
    let spent = started.elapsed();
    let names: Vec<&str> = items.iter().map(|item| &*item.author_name).collect();
    assert_eq!(
        names,
        [
            "author_7", "author_6", "author_5", "author_4", "author_3", "author_2", "author_1",
            "author_0"
        ],
        "восемь постов от новых к старым, у каждого имя автора: buffered сохраняет порядок"
    );
    assert!(
        spent >= PACE * 3,
        "страница (100 мс) и восемь авторов по 100 мс, не больше feed_lookups = 4 за раз: это два захода, всего 300 мс, а вышло {spent:?}: авторов спросили больше четырёх сразу"
    );
    assert!(
        spent < PACE * 4,
        "страница (100 мс) и восемь авторов по 100 мс, по четыре за раз: всего 300 мс, а вышло {spent:?}: похоже, авторов спрашивают по одному"
    );
}

#[tokio::test(start_paused = true)]
async fn feed_stream_is_lazy() {
    let (repos, reader) = readers_world(1, 40).await;
    let slow = SlowRepos::wrap(repos, PACE);
    let app = over(
        slow.repos(),
        AppConfig {
            feed_page: 4,
            ..AppConfig::default()
        },
    );
    let stream = app.feed_stream(reader);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        slow.calls(),
        0,
        "стрим ленив: пока его никто не тянет, хранилище не трогают"
    );
    let started = Instant::now();
    let first: Vec<PostItem> = within("два поста", stream.take(2).try_collect())
        .await
        .unwrap_or_else(|e| panic!("лента читается: {e:?}"));
    let spent = started.elapsed();
    assert_eq!(first.len(), 2, "take(2) берёт два поста");
    assert!(
        spent < PACE * 3,
        "двум постам хватает первой страницы и одного захода за автором (200 мс), а вышло {spent:?}"
    );
    let calls = slow.calls();
    assert!(
        calls <= 8,
        "take(2) из ленты на десять страниц: buffered держит впереди не больше четырёх постов, значит страниц не больше двух и запросов автора не больше шести, а хранилище спросили {calls} раз: стрим читает ленту впрок"
    );
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        slow.calls(),
        calls,
        "стрим брошен после take(2): дальше никто не читает страницы за его спиной"
    );
}

#[tokio::test]
async fn watch_shows_recent_history_then_live_posts() {
    let app = start(AppConfig::default());
    let alice = user(&app, "alice").await;
    let bob = user(&app, "bob").await;
    let carol = user(&app, "carol").await;
    follow(&app, bob, alice).await;
    post(&app, alice, "раз").await;
    let second = post(&app, alice, "два").await;
    let third = post(&app, alice, "три").await;

    let watch = app.watch(bob, 2);
    assert_stream(&watch);
    assert_eq!(
        app.watchers(),
        1,
        "watch подписывается на события сразу при вызове, ещё до первой страницы"
    );
    let mut watch = Box::pin(watch);
    let mut head = Vec::new();
    for _ in 0..3 {
        head.push(seen(&next(&mut watch).await));
    }
    assert_eq!(
        head,
        [Seen::History(third), Seen::History(second), Seen::UpToDate],
        "сначала два последних поста ленты, потом отметка UpToDate"
    );

    post(&app, carol, "мимо").await;
    let fourth = post(&app, alice, "четыре").await;
    let own = post(&app, bob, "своё").await;
    within(
        "закрытый пост",
        app.publish(alice, "только мне", Visibility::Private),
    )
    .await
    .expect("закрытый пост публикуется");

    let live = next(&mut watch).await;
    let FeedEvent::Live(item) = &live else {
        panic!("после UpToDate идут живые посты, а пришло {live:?}");
    };
    assert_eq!(
        (item.id, &*item.author_name, &*item.text),
        (fourth, "alice", "четыре"),
        "живой пост приходит целиком: id, имя автора, текст"
    );
    assert_eq!(
        seen(&next(&mut watch).await),
        Seen::Live(own),
        "свои посты тоже в ленте, значит и в живой"
    );
    assert!(
        timeout(Duration::from_millis(300), watch.next())
            .await
            .is_err(),
        "посты тех, на кого bob не подписан, и закрытые посты в живую ленту не попадают"
    );
}

async fn race(publish_after: Duration) -> (Vec<Seen>, PostId, PostId) {
    let repos = MemRepo::repos(MemNetwork::new());
    let setup = over(repos.clone(), AppConfig::default());
    let alice = user(&setup, "alice").await;
    let bob = user(&setup, "bob").await;
    follow(&setup, bob, alice).await;
    let first = post(&setup, alice, "старый пост").await;
    within("остановка", setup.close(DEADLINE)).await;

    let slow = SlowRepos::wrap(repos, PACE);
    let app = over(slow.repos(), AppConfig::default());
    let mut watch = Box::pin(app.watch(bob, 10));
    let publisher = app.clone();
    let late = tokio::spawn(async move {
        tokio::time::sleep(publish_after).await;
        publisher
            .publish(alice, "пока грузится история", Visibility::Public)
            .await
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let events = until_quiet(&mut watch, Duration::from_secs(1)).await;
    let late = within("поздний пост", late)
        .await
        .expect("задача публикации не падает")
        .expect("поздний пост публикуется");
    (events, first, late)
}

#[tokio::test(start_paused = true)]
async fn post_committed_before_the_first_page_comes_once() {
    let (events, first, late) = race(Duration::ZERO).await;
    assert_eq!(
        events,
        [Seen::History(late), Seen::History(first), Seen::UpToDate],
        "пост лёг в хранилище раньше, чем прочиталась первая страница: он уже в истории, и живая лента не должна прислать его второй раз"
    );
}

#[tokio::test(start_paused = true)]
async fn post_committed_after_the_first_page_comes_live() {
    let (events, first, late) = race(PACE / 2).await;
    assert_eq!(
        events,
        [Seen::History(first), Seen::UpToDate, Seen::Live(late)],
        "пост вышел после чтения первой страницы, но после вызова watch: в истории его нет, значит он приходит вживую (подписка берётся до первой страницы), и только после UpToDate"
    );
}

#[tokio::test]
async fn dropped_watch_stops_listening_and_close_ends_it() {
    let app = start(AppConfig::default());
    let alice = user(&app, "alice").await;
    let bob = user(&app, "bob").await;
    follow(&app, bob, alice).await;
    assert_eq!(app.watchers(), 0, "пока никто не смотрит, слушателей нет");
    let first = app.watch(bob, 5);
    let second = app.watch(alice, 5);
    assert_eq!(app.watchers(), 2, "каждый watch это свой слушатель");
    drop(first);
    assert_eq!(
        app.watchers(),
        1,
        "брошенный стрим отписывается сам, в Drop, без отдельной команды"
    );
    let mut second = Box::pin(second);
    assert_eq!(
        seen(&next(&mut second).await),
        Seen::UpToDate,
        "у alice лента пуста: сразу UpToDate"
    );
    within("остановка", app.close(DEADLINE)).await;
    assert!(
        within("конец живой ленты", second.next()).await.is_none(),
        "после close живая лента кончается, а не висит вечно"
    );
    drop(second);
    assert_eq!(app.watchers(), 0, "все стримы брошены, слушателей нет");
}
