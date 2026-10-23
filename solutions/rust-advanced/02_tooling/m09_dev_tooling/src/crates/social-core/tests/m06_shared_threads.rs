use std::borrow::Cow;
use std::thread::{self, JoinHandle};

use social_core::{
    DomainError, MemNetwork, MemPostStore, MemUserStore, Notification, PostId, SharedNetwork,
    UserId, Visibility, normalize_name,
};

type Shared = SharedNetwork<MemUserStore, MemPostStore>;

fn shared() -> Shared {
    SharedNetwork::new(MemNetwork::new()).expect("фоновый поток уведомлений должен запуститься")
}

fn user(net: &Shared, name: &str) -> UserId {
    net.register(name)
        .expect("регистрация нового ника должна пройти")
}

fn can_go_to_another_thread<T: Send + Clone + 'static>() {}

fn spawn<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> JoinHandle<T> {
    thread::Builder::new()
        .spawn(work)
        .expect("ОС должна дать тесту поток")
}

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

#[test]
fn clones_share_one_network() {
    let first = shared();
    let second = first.clone();
    let alice = user(&first, "alice");
    assert_eq!(
        second.read(|net| net.user(alice).map(|u| u.name().to_string())),
        Some(String::from("alice")),
        "клон ручки видит ту же сеть, а не копию"
    );
    assert_eq!(
        second.register("ALICE"),
        Err(DomainError::NameTaken(String::from("alice"))),
        "правила сети работают через любую ручку"
    );
}

#[test]
fn follow_lands_in_inbox() {
    let net = shared();
    let alice = user(&net, "alice");
    let bob = user(&net, "bob");
    assert_eq!(
        net.follow(alice, bob),
        Ok(true),
        "подписка проходит через ручку"
    );
    assert_eq!(
        net.inbox(bob),
        vec![Notification::Followed { by: alice }],
        "bob узнаёт, что alice на него подписалась"
    );
    assert!(net.inbox(alice).is_empty(), "у alice уведомлений нет");
    assert_eq!(
        net.follow(alice, bob),
        Ok(false),
        "повторная подписка ничего не меняет"
    );
    assert_eq!(
        net.inbox(bob).len(),
        1,
        "повторная подписка не шлёт второго уведомления"
    );
    assert_eq!(
        net.follow(bob, bob),
        Err(DomainError::SelfFollow),
        "ошибки сети проходят через ручку как есть"
    );
    assert_eq!(
        net.inbox(bob).len(),
        1,
        "после ошибки уведомлений не прибавилось"
    );
}

#[test]
fn like_lands_in_author_inbox() {
    let net = shared();
    let alice = user(&net, "alice");
    let bob = user(&net, "bob");
    let post = net
        .publish(alice, "лайкните", Visibility::Public)
        .expect("публикация должна пройти");
    assert_eq!(net.like(bob, post), Ok(true), "лайк проходит через ручку");
    assert_eq!(net.like(bob, post), Ok(false), "повторный лайк не засчитан");
    assert_eq!(net.like(alice, post), Ok(true), "свой пост лайкнуть можно");
    assert_eq!(
        net.inbox(alice),
        vec![Notification::Liked { by: bob, post }],
        "alice получает одно уведомление: от bob; повторный лайк и свой лайк уведомлений не шлют"
    );
    assert!(net.inbox(bob).is_empty(), "у bob уведомлений нет");
    assert_eq!(
        net.like(bob, PostId(99)),
        Err(DomainError::UnknownPost(PostId(99))),
        "ошибка лайка проходит как есть"
    );
}

#[test]
fn inbox_sees_everything_sent_before_it() {
    let net = shared();
    let star = user(&net, "star");
    let post = net
        .publish(star, "звёздный пост", Visibility::Public)
        .expect("публикация должна пройти");
    for i in 0..200 {
        let fan = user(&net, &format!("fan{i}"));
        net.follow(fan, star).expect("подписка проходит");
        net.like(fan, post).expect("лайк проходит");
    }
    assert_eq!(
        net.inbox(star).len(),
        400,
        "inbox должен дождаться, пока фоновый поток разложит всё, что отправили до вызова"
    );
}

#[test]
fn read_gives_access_to_the_feed() {
    let net = shared();
    let alice = user(&net, "alice");
    let bob = user(&net, "bob");
    net.follow(alice, bob).expect("подписка проходит");
    net.publish(bob, "из соседнего потока", Visibility::Followers)
        .expect("публикация должна пройти");
    assert_eq!(
        net.read(|n| n.feed(alice, 10).len()),
        1,
        "через read видна лента alice"
    );
}

#[test]
fn ten_threads_post_follow_and_like_at_once() {
    let net = shared();
    let mut writers = Vec::new();
    for i in 0..10 {
        let net = net.clone();
        writers.push(spawn(move || {
            let me = net
                .register(&format!("user{i}"))
                .expect("у каждого потока свой ник");
            let posts: Vec<PostId> = (0..3)
                .map(|n| {
                    net.publish(me, &format!("пост {n} от user{i}"), Visibility::Public)
                        .expect("публикация должна пройти")
                })
                .collect();
            (me, posts)
        }));
    }
    let people: Vec<(UserId, Vec<PostId>)> = writers
        .into_iter()
        .map(|handle| {
            handle
                .join()
                .expect("поток публикации не должен паниковать")
        })
        .collect();

    let mut fans = Vec::new();
    for (me, _) in &people {
        let me = *me;
        let net = net.clone();
        let others = people.clone();
        fans.push(spawn(move || {
            for (other, posts) in others.iter().filter(|(other, _)| *other != me) {
                net.follow(me, *other).expect("подписка проходит");
                for post in posts {
                    net.like(me, *post).expect("лайк проходит");
                }
            }
        }));
    }
    for handle in fans {
        handle.join().expect("поток лайков не должен паниковать");
    }

    assert_eq!(
        net.read(|n| n.user_count()),
        10,
        "десять потоков зарегистрировали десять пользователей"
    );
    assert_eq!(
        net.read(|n| n.post_count()),
        30,
        "каждый из десяти опубликовал по три поста"
    );
    for (me, posts) in &people {
        assert_eq!(
            net.read(|n| n.followers_count(*me)),
            9,
            "на каждого подписались остальные девять"
        );
        for post in posts {
            assert_eq!(
                net.read(|n| n.likes(*post)),
                9,
                "каждый пост лайкнули девять других, ни один лайк не потерялся"
            );
        }
        let inbox = net.inbox(*me);
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
