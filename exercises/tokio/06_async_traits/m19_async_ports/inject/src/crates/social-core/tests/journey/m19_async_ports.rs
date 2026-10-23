use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use contracts::{COMMAND_METHODS, Command, Username};
use social_core::{
    App, AppConfig, AppError, ChatRepo, CommandError, CommandTable, IntoResponse, MemNetwork,
    MemRepo, Network, Notifiers, Op, PostId, PostRepo, Repos, UserId, UserRepo, Visibility,
    execute_line, social_table,
};
use test_support::{FakePosts, FakeUsers, SlowRepos};
use tokio::time::{Instant, timeout};

const DEADLINE: Duration = Duration::from_secs(10);

const DELAY: Duration = Duration::from_millis(100);

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn on(repos: Repos, config: AppConfig) -> App {
    App::with_repos(repos, Notifiers::new(), config).expect("ядро запускается внутри рантайма")
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

fn ports(_: Arc<dyn UserRepo>, _: Arc<dyn PostRepo>, _: Arc<dyn ChatRepo>) {}

#[tokio::test]
async fn app_is_built_from_ports() {
    let repos = MemRepo::repos(MemNetwork::new());
    ports(
        repos.users.clone(),
        repos.posts.clone(),
        repos.chats.clone(),
    );
    let net = on(repos.clone(), AppConfig::default());
    let alice = user(&net, "alice").await;
    assert_eq!(
        repos
            .users
            .user(alice)
            .await
            .map(|user| user.name().to_string()),
        Some(String::from("alice")),
        "App пишет в те же порты, что ему дали: Arc<dyn UserRepo> один на двоих"
    );
    assert_eq!(repos.users.count().await, 1);
}

#[tokio::test]
async fn ports_hide_the_storage() {
    let network = Network::with_stores(FakeUsers::starting_at(100), FakePosts::starting_at(500));
    let net = on(MemRepo::repos(network), AppConfig::default());
    let alice = user(&net, "alice").await;
    let first = post(&net, alice, "через порты").await;
    assert_eq!(
        (alice, first),
        (UserId(100), PostId(500)),
        "App не знает, что под портами: id выдают фейковые хранилища"
    );
}

fn send<T: Send>(_: T) {}

#[tokio::test]
async fn all_app_futures_are_send() {
    let net = on(MemRepo::repos(MemNetwork::new()), AppConfig::default());
    let id = UserId(1);
    let post = PostId(1);
    let comment = social_core::CommentId(1);
    send(net.register("alice"));
    send(net.publish(id, "текст", Visibility::Public));
    send(net.follow(id, id));
    send(net.like(id, post));
    send(net.mute(id, id));
    send(net.delete_post(id, post));
    send(net.comment(id, post, "текст"));
    send(net.reply(id, comment, "текст"));
    send(net.send_message(id, id, "текст"));
    send(net.history(id, id, 10));
    send(net.inbox(id));
    send(net.flush());
    send(net.profile(id));
    send(net.user(id));
    send(net.find_user("alice"));
    send(net.user_count());
    send(net.post_count());
    send(net.is_following(id, id));
    send(net.follow_counts(id));
    send(net.author_stats(id));
    send(net.post(post));
    send(net.feed(id, 0, 10));
    send(net.author_page(id, id, None, 10));
    send(net.thread(id, post));
    send(net.card(id, id));
    send(net.refresh_trends());
    send(net.import(String::new()));
    send(net.import_file("seed.txt"));
    send(net.run_heavy(Op::Job("проверка"), || 1));
    send(execute_line("feed alice", &net));
    send(social_table().call(&net, Command::Stats));
}

#[tokio::test(start_paused = true)]
async fn card_parts_go_in_parallel() {
    let slow = SlowRepos::over(MemNetwork::new(), DELAY);
    let net = on(slow.repos(), AppConfig::default());
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(bob, alice).await.expect("подписка проходит");
    post(&net, alice, "раз").await;
    let started = Instant::now();
    let card = within("карточка", net.card(bob, alice))
        .await
        .expect("карточка собирается");
    let took = started.elapsed();
    assert_eq!(card.followers, 1);
    assert!(card.followed);
    assert!(
        took < DELAY * 2,
        "у карточки четыре части и каждый порт отвечает за {DELAY:?}, а собралась она за {took:?}: \
         части должны идти одновременно (try_join!), а не одна за другой"
    );
}

#[tokio::test(start_paused = true)]
async fn card_misses_the_deadline_on_slow_storage() {
    let slow = SlowRepos::over(MemNetwork::new(), Duration::from_secs(1));
    let net = on(
        slow.repos(),
        AppConfig {
            op_timeout: Duration::from_millis(300),
            ..AppConfig::default()
        },
    );
    let alice = user(&net, "alice").await;
    assert_eq!(
        within("карточка", net.card(alice, alice)).await,
        Err(AppError::Timeout {
            op: Op::Card,
            after: Duration::from_millis(300),
        }),
        "хранилище отвечает секунду, а у карточки дедлайн 300 мс"
    );
}

async fn comment_tree(net: &App) -> (UserId, PostId, Vec<(usize, u64)>) {
    let alice = user(net, "alice").await;
    let bob = user(net, "bob").await;
    let post = post(net, alice, "обсудим").await;
    let first = net.comment(bob, post, "первый").await.expect("комментарий");
    let deep = net.reply(alice, first, "ответ").await.expect("ответ");
    net.reply(bob, deep, "ещё глубже").await.expect("ответ");
    let second = net
        .comment(alice, post, "второй")
        .await
        .expect("комментарий");
    net.reply(bob, first, "второй ответ").await.expect("ответ");
    let expected = vec![(0, first.0), (1, deep.0), (2, 3), (1, 5), (0, second.0)];
    (bob, post, expected)
}

#[tokio::test]
async fn thread_loads_branches_recursively() {
    let net = on(MemRepo::repos(MemNetwork::new()), AppConfig::default());
    let (bob, post, expected) = comment_tree(&net).await;
    let thread = within("ветка", net.thread(bob, post))
        .await
        .expect("ветка читается");
    let got: Vec<(usize, u64)> = thread
        .iter()
        .map(|(depth, comment)| (*depth, comment.id().0))
        .collect();
    assert_eq!(
        got, expected,
        "ветка обходится в глубину: комментарий, все его ответы со своей глубиной, потом следующий"
    );
    assert_eq!(
        within(
            "thread",
            execute_line(&format!("thread bob {}", post.0), &net)
        )
        .await,
        Ok(String::from(
            "[1] @bob: первый\n  [2] @alice: ответ\n    [3] @bob: ещё глубже\n  [5] @bob: второй ответ\n[4] @alice: второй"
        )),
        "команда thread печатает ту же ветку с отступами"
    );
}

#[tokio::test(start_paused = true)]
async fn thread_asks_the_port_once_per_comment() {
    let slow = SlowRepos::over(MemNetwork::new(), Duration::from_millis(1));
    let net = on(slow.repos(), AppConfig::default());
    let (bob, post, expected) = comment_tree(&net).await;
    let before = slow.calls();
    within("ветка", net.thread(bob, post))
        .await
        .expect("ветка читается");
    assert_eq!(
        slow.calls() - before,
        expected.len() + 1,
        "рекурсивная async fn спрашивает ответы у каждого комментария и у самого поста: \
         {} комментариев, {} вызовов порта",
        expected.len(),
        expected.len() + 1
    );
}

#[tokio::test]
async fn table_routes_every_command() {
    let mut methods = COMMAND_METHODS.to_vec();
    methods.sort_unstable();
    assert_eq!(
        social_table().methods(),
        methods,
        "таблица ядра знает каждый метод из COMMAND_METHODS"
    );
    assert_eq!(CommandTable::social().methods(), methods);
}

async fn shout(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Register(name) = command else {
        return Err(CommandError::NoRoute("users.register"));
    };
    let id = app.register(name.as_ref()).await?;
    Ok(format!("ПРИВЕТ, {name}! id {}", id.0))
}

#[tokio::test]
async fn table_accepts_plain_async_fns() {
    let net = on(MemRepo::repos(MemNetwork::new()), AppConfig::default());
    let mut table = CommandTable::new();
    assert!(
        table.route("users.register", shout),
        "первая регистрация метода"
    );
    assert!(
        !table.route("users.register", shout),
        "повторная регистрация заменяет обработчик и говорит об этом"
    );
    let name = Username::try_from("alice").expect("ник");
    assert_eq!(
        within("свой обработчик", table.call(&net, Command::Register(name))).await,
        Ok(String::from("ПРИВЕТ, @alice! id 1")),
        "обработчик это обычная async fn (App, Command) -> Result"
    );
    let missing = within("нет метода", table.call(&net, Command::Stats))
        .await
        .expect_err("admin.stats не зарегистрирован");
    assert_eq!(missing, CommandError::NoRoute("admin.stats"));
    assert_eq!(missing.into_response().status, 500);
}
