use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use memory::{MemPosts, MemUsers, SlowPosts, SlowUsers};
use ports::{Commands, PostRepo, RepoError, Response, Service, UserRepo};
use tokio::time::{Instant, sleep};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn mem_service() -> Service {
    Service::new(Arc::new(MemUsers::new()), Arc::new(MemPosts::new()))
}

fn slow_service(delay: u64) -> Service {
    let users: Arc<dyn UserRepo> = Arc::new(SlowUsers::new(Arc::new(MemUsers::new()), ms(delay)));
    let posts: Arc<dyn PostRepo> = Arc::new(SlowPosts::new(Arc::new(MemPosts::new()), ms(delay)));
    Service::new(users, posts)
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn assert_send<T: Send>(_future: &T) {}

#[tokio::test]
async fn memory_users_behave_like_a_table() {
    let users = MemUsers::new();
    let ann = users.create("ann".to_string()).await.unwrap();
    let bob = users.create("bob".to_string()).await.unwrap();
    assert_eq!((ann.id, bob.id), (1, 2), "идентификаторы выдаются подряд с единицы");
    assert_eq!(users.get(2).await, Ok(bob), "get отдаёт сохранённого пользователя целиком");
    assert_eq!(users.get(9).await, Err(RepoError::NotFound), "нет такого пользователя: NotFound");
    assert_eq!(users.create("ann".to_string()).await, Err(RepoError::Conflict), "имя занято: Conflict");
    assert_eq!(users.count().await, 2, "неудачное создание не должно менять счёт");
}

#[tokio::test]
async fn memory_posts_are_listed_per_author_in_order() {
    let posts = MemPosts::new();
    let first = posts.add(1, "раз".to_string()).await.unwrap();
    posts.add(2, "чужой".to_string()).await.unwrap();
    let third = posts.add(1, "два".to_string()).await.unwrap();
    assert_eq!((first.id, third.id), (1, 3), "идентификаторы постов общие и растут подряд");
    let ids: Vec<u32> = posts.by_author(1).await.iter().map(|post| post.id).collect();
    assert_eq!(ids, vec![1, 3], "у автора 1 два поста, в порядке добавления");
    assert!(posts.by_author(7).await.is_empty(), "у незнакомого автора постов нет");
}

#[tokio::test(start_paused = true)]
async fn slow_decorators_delay_every_call_and_change_nothing_else() {
    let slow: Arc<dyn UserRepo> = Arc::new(SlowUsers::new(Arc::new(MemUsers::new()), ms(40)));
    let start = Instant::now();
    let user = slow.create("ann".to_string()).await.unwrap();
    assert_eq!(start.elapsed(), ms(40), "каждый вызов ждёт задержку один раз");
    assert_eq!(slow.get(user.id).await, Ok(user), "ответ внутреннего хранилища передаётся как есть");
    assert_eq!(slow.get(5).await, Err(RepoError::NotFound), "ошибки тоже передаются");
    assert_eq!(slow.count().await, 1, "счёт берётся из внутреннего хранилища");
    assert_eq!(start.elapsed(), ms(160), "четыре вызова по 40 мс");

    let slow: Arc<dyn PostRepo> = Arc::new(SlowPosts::new(Arc::new(MemPosts::new()), ms(10)));
    slow.add(1, "x".to_string()).await.unwrap();
    assert_eq!(slow.by_author(1).await.len(), 1, "посты проходят через обёртку");
}

#[tokio::test]
async fn service_checks_the_author_before_publishing() {
    let service = mem_service();
    assert_eq!(service.publish(1, "привет").await, Err(RepoError::NotFound), "автора нет: публиковать нельзя");
    let user = service.register("ann").await.unwrap();
    assert_eq!(service.register("ann").await, Err(RepoError::Conflict), "повторная регистрация имени");
    let post = service.publish(user.id, "привет").await.unwrap();
    assert_eq!((post.author, post.body.as_str()), (user.id, "привет"), "пост принадлежит автору");
    let profile = service.profile(user.id).await.unwrap();
    assert_eq!(profile.user, user, "в профиле пользователь");
    assert_eq!(profile.posts, vec![post], "и его единственный пост, чужие и отвергнутые посты не попали");
    assert_eq!(service.profile(77).await, Err(RepoError::NotFound), "профиль несуществующего пользователя");
}

#[tokio::test(start_paused = true)]
async fn profile_asks_both_ports_at_the_same_time() {
    let service = slow_service(50);
    let user = service.register("ann").await.unwrap();
    service.publish(user.id, "x").await.unwrap();
    let start = Instant::now();
    let profile = service.profile(user.id).await.unwrap();
    assert_eq!(profile.posts.len(), 1, "профиль собран");
    assert_eq!(start.elapsed(), ms(50), "пользователь и посты запрашиваются вместе: 50 мс, а не 100");
}

#[tokio::test(start_paused = true)]
async fn rejected_publish_does_not_touch_the_post_port() {
    let service = slow_service(50);
    let start = Instant::now();
    assert_eq!(service.publish(1, "x").await, Err(RepoError::NotFound), "автора нет");
    assert_eq!(start.elapsed(), ms(50), "после отказа по автору к постам обращаться незачем: только запрос пользователя");
}

#[tokio::test]
async fn all_method_futures_are_send() {
    let service = Arc::new(mem_service());
    assert_send(&service.register("x"));
    assert_send(&service.publish(1, "x"));
    assert_send(&service.profile(1));
    let commands = Commands::new(Arc::clone(&service));
    assert_send(&commands.dispatch("user.profile", args(&["1"])));
    let handle = tokio::spawn({
        let service = Arc::clone(&service);
        async move { service.register("spawned").await }
    });
    assert!(handle.await.unwrap().is_ok(), "метод сервиса должен запускаться отдельной задачей");
}

#[tokio::test]
async fn commands_drive_the_service() {
    let service = Arc::new(mem_service());
    let commands = Commands::new(Arc::clone(&service));
    assert_eq!(commands.dispatch("user.register", args(&["ann"])).await, Response::Ok("1".into()), "регистрация возвращает id");
    assert!(matches!(commands.dispatch("user.register", args(&["ann"])).await, Response::Error(_)), "занятое имя: ошибка");
    assert!(matches!(commands.dispatch("user.register", args(&[])).await, Response::Error(_)), "без аргумента: ошибка");
    assert_eq!(commands.dispatch("post.publish", args(&["1", "привет"])).await, Response::Ok("1".into()), "публикация возвращает id поста");
    assert!(matches!(commands.dispatch("post.publish", args(&["x", "привет"])).await, Response::Error(_)), "нечисловой автор: ошибка");
    assert!(matches!(commands.dispatch("post.publish", args(&["9", "привет"])).await, Response::Error(_)), "нет автора: ошибка");
    assert_eq!(commands.dispatch("user.profile", args(&["1"])).await, Response::Ok("ann:1".into()), "профиль: имя и число постов");
    assert_eq!(commands.dispatch("no.such", args(&[])).await, Response::UnknownCommand, "неизвестная команда");
}

#[tokio::test(start_paused = true)]
async fn custom_handlers_can_be_added_and_called_from_many_tasks() {
    let service = Arc::new(slow_service(20));
    let mut commands = Commands::new(Arc::clone(&service));
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&hits);
    commands.register("debug.ping", move |_args| {
        let seen = Arc::clone(&seen);
        async move {
            sleep(ms(30)).await;
            Response::Ok(seen.fetch_add(1, Ordering::SeqCst).to_string())
        }
    });
    let commands = Arc::new(commands);
    let start = Instant::now();
    let tasks: Vec<_> = (0..5)
        .map(|_| {
            let commands = Arc::clone(&commands);
            tokio::spawn(async move { commands.dispatch("debug.ping", vec![]).await })
        })
        .collect();
    for task in tasks {
        assert!(matches!(task.await.unwrap(), Response::Ok(_)), "свой обработчик отвечает");
    }
    assert_eq!(hits.load(Ordering::SeqCst), 5, "каждый вызов дошёл до обработчика");
    assert_eq!(start.elapsed(), ms(30), "пять вызовов шли вместе");
    assert_eq!(commands.dispatch("user.register", args(&["bob"])).await, Response::Ok("1".into()), "стандартные команды остаются на месте");
}
