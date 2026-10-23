use std::borrow::Cow;
use std::future::Future;
use std::time::Duration;

use social_core::{
    App, AppError, DomainError, MemNetwork, Notification, PostId, UserId, Visibility,
    normalize_name,
};
use tokio::task::JoinSet;
use tokio::time::timeout;

type Shared = App;

const DEADLINE: Duration = Duration::from_secs(10);

fn shared() -> Shared {
    App::new(MemNetwork::new()).expect("ядро запускается внутри рантайма")
}

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

async fn user(net: &Shared, name: &str) -> UserId {
    net.register(name)
        .await
        .expect("регистрация нового ника должна пройти")
}

async fn inbox(net: &Shared, user: UserId) -> Vec<Notification> {
    within("inbox", net.inbox(user))
        .await
        .expect("уведомления доставляются вовремя")
}

async fn join_all<T: Send + 'static>(what: &str, mut tasks: JoinSet<T>) -> Vec<T> {
    within(what, async move {
        let mut done = Vec::new();
        while let Some(result) = tasks.join_next().await {
            done.push(result.unwrap_or_else(|e| panic!("{what}: задача упала: {e}")));
        }
        done
    })
    .await
}

fn can_go_to_another_thread<T: Send + Sync + Clone + 'static>() {}

#[test]
fn normalize_name_does_not_allocate_without_need() {
    assert!(
        matches!(normalize_name("alice"), Cow::Borrowed("alice")),
        "уже нормальный ник отдаётся как Cow::Borrowed, без новой строки"
    );
    assert!(
        matches!(normalize_name("  bob  "), Cow::Borrowed("bob")),
        "обрезать пробелы можно без аллокации: trim отдаёт кусок той же строки"
    );
    assert!(
        matches!(normalize_name(" Carol "), Cow::Owned(ref name) if name == "carol"),
        "заглавные буквы требуют новой строки: Cow::Owned(\"carol\")"
    );
}

#[test]
fn shared_network_moves_between_threads() {
    can_go_to_another_thread::<Shared>();
}

#[tokio::test]
async fn clones_share_one_network() {
    let first = shared();
    let second = first.clone();
    let alice = user(&first, "alice").await;
    assert_eq!(
        second.user(alice).await.map(|u| u.name().to_string()),
        Some(String::from("alice")),
        "клон ручки видит ту же сеть, а не копию"
    );
    assert_eq!(
        second.register("ALICE").await,
        Err(AppError::Domain(DomainError::NameTaken(String::from(
            "alice"
        )))),
        "правила сети работают через любую ручку"
    );
}

#[tokio::test]
async fn follow_lands_in_inbox() {
    let net = shared();
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    assert_eq!(
        net.follow(alice, bob).await,
        Ok(true),
        "подписка проходит через ручку"
    );
    assert_eq!(
        inbox(&net, bob).await,
        vec![Notification::Followed { by: alice }],
        "bob узнаёт, что alice на него подписалась"
    );
    assert!(
        inbox(&net, alice).await.is_empty(),
        "у alice уведомлений нет"
    );
    assert_eq!(
        net.follow(alice, bob).await,
        Ok(false),
        "повторная подписка ничего не меняет"
    );
    assert_eq!(
        inbox(&net, bob).await.len(),
        1,
        "повторная подписка не шлёт второго уведомления"
    );
    assert_eq!(
        net.follow(bob, bob).await,
        Err(AppError::Domain(DomainError::SelfFollow)),
        "ошибки сети проходят через ручку как AppError::Domain"
    );
    assert_eq!(
        inbox(&net, bob).await.len(),
        1,
        "после ошибки уведомлений не прибавилось"
    );
}

#[tokio::test]
async fn like_lands_in_author_inbox() {
    let net = shared();
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    let post = net
        .publish(alice, "лайкните", Visibility::Public)
        .await
        .expect("публикация должна пройти");
    assert_eq!(
        net.like(bob, post).await,
        Ok(true),
        "лайк проходит через ручку"
    );
    assert_eq!(
        net.like(bob, post).await,
        Ok(false),
        "повторный лайк не засчитан"
    );
    assert_eq!(
        net.like(alice, post).await,
        Ok(true),
        "свой пост лайкнуть можно"
    );
    assert_eq!(
        inbox(&net, alice).await,
        vec![Notification::Liked { by: bob, post }],
        "alice получает одно уведомление: от bob; повторный лайк и свой лайк уведомлений не шлют"
    );
    assert!(inbox(&net, bob).await.is_empty(), "у bob уведомлений нет");
    assert_eq!(
        net.like(bob, PostId(99)).await,
        Err(AppError::Domain(DomainError::UnknownPost(PostId(99)))),
        "ошибка лайка проходит как есть"
    );
}

#[tokio::test]
async fn inbox_sees_everything_sent_before_it() {
    let net = shared();
    let star = user(&net, "star").await;
    let post = net
        .publish(star, "звёздный пост", Visibility::Public)
        .await
        .expect("публикация должна пройти");
    for i in 0..200 {
        let fan = user(&net, &format!("fan{i}")).await;
        net.follow(fan, star).await.expect("подписка проходит");
        net.like(fan, post).await.expect("лайк проходит");
    }
    assert_eq!(
        inbox(&net, star).await.len(),
        400,
        "inbox видит всё, что отправили до вызова"
    );
}

#[tokio::test]
async fn read_gives_access_to_the_feed() {
    let net = shared();
    let alice = user(&net, "alice").await;
    let bob = user(&net, "bob").await;
    net.follow(alice, bob).await.expect("подписка проходит");
    net.publish(bob, "из соседней задачи", Visibility::Followers)
        .await
        .expect("публикация должна пройти");
    assert_eq!(
        net.feed(alice, 0, usize::MAX).await.len(),
        1,
        "лента alice видна через App::feed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ten_tasks_post_follow_and_like_at_once() {
    let net = shared();
    let mut writers = JoinSet::new();
    for i in 0..10 {
        let net = net.clone();
        writers.spawn(async move {
            let me = net
                .register(&format!("user{i}"))
                .await
                .expect("у каждой задачи свой ник");
            let mut posts = Vec::new();
            for n in 0..3 {
                let post = net
                    .publish(me, &format!("пост {n} от user{i}"), Visibility::Public)
                    .await
                    .expect("публикация должна пройти");
                posts.push(post);
            }
            (me, posts)
        });
    }
    let people: Vec<(UserId, Vec<PostId>)> = join_all("публикации", writers).await;

    let mut fans = JoinSet::new();
    for (me, _) in &people {
        let me = *me;
        let net = net.clone();
        let others = people.clone();
        fans.spawn(async move {
            for (other, posts) in others.iter().filter(|(other, _)| *other != me) {
                net.follow(me, *other).await.expect("подписка проходит");
                for post in posts {
                    net.like(me, *post).await.expect("лайк проходит");
                }
            }
        });
    }
    join_all("подписки и лайки", fans).await;

    assert_eq!(
        net.user_count().await,
        10,
        "десять задач зарегистрировали десять пользователей"
    );
    assert_eq!(
        net.post_count().await,
        30,
        "каждый из десяти опубликовал по три поста"
    );
    for (me, posts) in &people {
        assert_eq!(
            net.follow_counts(*me).await.followers,
            9,
            "на каждого подписались остальные девять"
        );
        for post in posts {
            assert_eq!(
                net.post(*post).await.map_or(0, |post| post.likes),
                9,
                "каждый пост лайкнули девять других, ни один лайк не потерялся"
            );
        }
        let inbox = inbox(&net, *me).await;
        let follows = inbox
            .iter()
            .filter(|n| matches!(n, Notification::Followed { .. }))
            .count();
        let likes = inbox
            .iter()
            .filter(|n| matches!(n, Notification::Liked { .. }))
            .count();
        assert_eq!(follows, 9, "в ящике 9 уведомлений о подписке");
        assert_eq!(
            likes, 27,
            "в ящике 27 уведомлений о лайках: 9 человек на 3 поста"
        );
    }
}
