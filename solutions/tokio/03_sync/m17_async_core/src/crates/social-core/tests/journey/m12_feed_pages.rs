use std::cell::Cell;

use social_core::{
    App, CommandError, DomainError, MemNetwork, MemPostStore, MemUserStore, PagesExt, Post, PostId,
    UserId, Visibility, execute_line,
};

struct World {
    net: MemNetwork,
    alice: UserId,
    bob: UserId,
    carol: UserId,
}

fn world() -> World {
    let mut net = MemNetwork::new();
    let alice = net.register("alice").expect("alice регистрируется");
    let bob = net.register("bob").expect("bob регистрируется");
    let carol = net.register("carol").expect("carol регистрируется");
    net.follow(alice, bob).expect("alice подписывается на bob");
    net.follow(alice, carol)
        .expect("alice подписывается на carol");
    net.follow(carol, bob).expect("carol подписывается на bob");
    World {
        net,
        alice,
        bob,
        carol,
    }
}

fn publish(w: &mut World, author: UserId, text: &str) -> PostId {
    w.net
        .publish(author, text, Visibility::Public)
        .expect("публикация проходит")
}

fn ids<'a>(posts: impl Iterator<Item = &'a Post>) -> Vec<PostId> {
    posts.map(Post::id).collect()
}

struct Flaky {
    step: u32,
}

impl Iterator for Flaky {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        self.step += 1;
        match self.step {
            1 | 3 => Some(self.step),
            _ => None,
        }
    }
}

#[test]
fn pages_split_any_iterator() {
    assert_eq!(
        (1..=7).pages(3).collect::<Vec<_>>(),
        [vec![1, 2, 3], vec![4, 5, 6], vec![7]],
        "последняя страница может быть неполной"
    );
    assert_eq!(
        (1..=6).pages(3).collect::<Vec<_>>(),
        [vec![1, 2, 3], vec![4, 5, 6]],
        "пустой страницы в конце не бывает"
    );
    assert_eq!(
        std::iter::empty::<u32>().pages(3).next(),
        None,
        "у пустого итератора страниц нет"
    );
    assert_eq!(
        (1..=5).pages(0).next(),
        None,
        "страниц размером 0 нет, и паники тоже нет"
    );
    assert_eq!(
        "раз два три"
            .split(' ')
            .pages(2)
            .map(|page| page.join("+"))
            .collect::<Vec<_>>(),
        ["раз+два", "три"],
        "pages есть у любого итератора: это трейт-расширение с общей реализацией"
    );
}

#[test]
fn pages_are_lazy() {
    let pulled = Cell::new(0);
    let mut pages = (1..).inspect(|_| pulled.set(pulled.get() + 1)).pages(3);
    assert_eq!(
        pages.next(),
        Some(vec![1, 2, 3]),
        "бесконечный источник режется на страницы"
    );
    assert_eq!(
        pulled.get(),
        3,
        "за одну страницу из источника берётся ровно одна страница элементов"
    );
    assert_eq!(
        pages.nth(1),
        Some(vec![7, 8, 9]),
        "nth(1) пропускает одну страницу и отдаёт следующую"
    );
    assert_eq!(pulled.get(), 9, "пропуск страницы не читает лишнего");
    assert_eq!(
        (0..10).pages(usize::MAX).nth(2),
        None,
        "огромный номер страницы не роняет программу переполнением"
    );
}

#[test]
fn pages_stop_after_the_end() {
    let mut pages = Flaky { step: 0 }.pages(2);
    assert_eq!(
        pages.next(),
        Some(vec![1]),
        "первая страница до первого None"
    );
    assert_eq!(pages.next(), None, "после конца страниц нет");
    assert_eq!(
        pages.next(),
        None,
        "исчерпанный источник больше не опрашивается: адаптер держит его через fuse"
    );
}

#[test]
fn feed_is_a_lazy_iterator() {
    let mut w = world();
    let (alice, bob, carol) = (w.alice, w.bob, w.carol);
    let b1 = publish(&mut w, bob, "b1");
    let c1 = publish(&mut w, carol, "c1");
    let a1 = publish(&mut w, alice, "a1");
    let b2 = publish(&mut w, bob, "b2");
    assert_eq!(
        ids(w.net.feed(alice)),
        [b2, a1, c1, b1],
        "лента alice: свои и подписки, новые сверху"
    );
    assert_eq!(
        ids(w.net.feed(alice).take(2)),
        [b2, a1],
        "сколько взять, решает вызывающий"
    );
    let pages: Vec<Vec<PostId>> = w
        .net
        .feed(alice)
        .pages(3)
        .map(|page| page.into_iter().map(Post::id).collect())
        .collect();
    assert_eq!(
        pages,
        [vec![b2, a1, c1], vec![b1]],
        "лента режется на страницы своим адаптером"
    );
    let first = w.net.feed(alice).next().expect("лента не пустая");
    let stored = w.net.post(b2).expect("пост лежит в хранилище");
    assert!(
        std::ptr::eq(first, stored),
        "лента отдаёт ссылки на посты хранилища"
    );
}

#[test]
fn muted_authors_leave_feed_and_search() {
    let mut w = world();
    let (alice, bob, carol) = (w.alice, w.bob, w.carol);
    let b1 = publish(&mut w, bob, "bob #rust");
    let c1 = publish(&mut w, carol, "carol #rust");
    assert_eq!(w.net.mute(alice, bob), Ok(true), "alice скрывает bob");
    assert_eq!(
        w.net.mute(alice, bob),
        Ok(false),
        "повторное скрытие ничего не меняет"
    );
    assert!(w.net.is_muted(alice, bob), "is_muted видит скрытие");
    assert!(!w.net.is_muted(bob, alice), "скрытие не взаимное");
    assert_eq!(
        ids(w.net.feed(alice)),
        [c1],
        "скрытого автора нет в ленте, хотя подписка осталась"
    );
    assert!(w.net.is_following(alice, bob), "mute не отменяет подписку");
    assert_eq!(
        ids(w.net.search_tag(alice, "rust")),
        [c1],
        "скрытого автора нет и в поиске"
    );
    assert_eq!(
        ids(w.net.feed(carol)),
        [c1, b1],
        "у carol лента прежняя: скрытие касается только alice"
    );
    assert_eq!(
        w.net.mute(alice, alice),
        Err(DomainError::SelfMute),
        "себя скрыть нельзя"
    );
    assert_eq!(
        w.net.mute(alice, UserId(99)),
        Err(DomainError::UnknownUser(UserId(99))),
        "незнакомца скрыть нельзя"
    );
}

#[test]
fn deleted_posts_are_soft_deleted() {
    let mut w = world();
    let (alice, bob, carol) = (w.alice, w.bob, w.carol);
    let gone = publish(&mut w, bob, "удалю #rust");
    let kept = publish(&mut w, bob, "оставлю #rust");
    w.net.like(carol, gone).expect("лайк до удаления проходит");
    w.net.like(carol, kept).expect("лайк проходит");
    assert_eq!(
        w.net.delete_post(alice, gone),
        Err(DomainError::NotAuthor(gone)),
        "чужой пост удалить нельзя"
    );
    assert_eq!(
        w.net.delete_post(bob, gone),
        Ok(true),
        "автор удаляет свой пост"
    );
    assert_eq!(
        w.net.delete_post(bob, gone),
        Ok(false),
        "второй раз удалять нечего"
    );
    assert_eq!(
        w.net.delete_post(bob, PostId(99)),
        Err(DomainError::UnknownPost(PostId(99))),
        "нет такого поста"
    );
    assert_eq!(
        w.net.delete_post(UserId(99), kept),
        Err(DomainError::UnknownUser(UserId(99))),
        "незнакомец ничего не удаляет"
    );
    assert_eq!(
        ids(w.net.feed(alice)),
        [kept],
        "удалённого поста нет в ленте"
    );
    assert_eq!(
        ids(w.net.search_tag(carol, "rust")),
        [kept],
        "удалённого поста нет в поиске"
    );
    assert!(
        w.net.post(gone).is_some_and(Post::is_deleted),
        "удаление мягкое: пост остался в хранилище с пометкой is_deleted"
    );
    assert_eq!(
        w.net.like(alice, gone),
        Err(DomainError::UnknownPost(gone)),
        "удалённый пост лайкнуть нельзя"
    );
    assert_eq!(
        w.net.post_count_of(bob),
        1,
        "удалённые посты автору не засчитываются"
    );
    assert_eq!(w.net.likes_received(bob), 1, "лайки удалённых постов тоже");
}

#[test]
fn new_errors_read_like_sentences() {
    assert_eq!(
        DomainError::SelfMute.to_string(),
        "нельзя скрыть самого себя",
        "Display для SelfMute"
    );
    assert_eq!(
        DomainError::NotAuthor(PostId(3)).to_string(),
        "удалить пост [3] может только автор",
        "Display для NotAuthor"
    );
}

#[tokio::test]
async fn shared_network_mutes_and_deletes() {
    let net = App::new(MemNetwork::new()).expect("ядро запускается внутри рантайма");
    let alice = net.register("alice").await.expect("alice регистрируется");
    let bob = net.register("bob").await.expect("bob регистрируется");
    let post = net
        .publish(bob, "пост", Visibility::Public)
        .await
        .expect("публикация проходит");
    assert_eq!(net.mute(alice, bob).await, Ok(true), "mute через ручку");
    assert_eq!(
        net.delete_post(bob, post).await,
        Ok(true),
        "удаление через ручку"
    );
    assert!(
        net.read(|network| network.post(post).is_some_and(Post::is_deleted)),
        "ручка меняет ту же сеть"
    );
}

#[tokio::test]
async fn commands_page_mute_and_delete() {
    let net: App<MemUserStore, MemPostStore> =
        App::new(MemNetwork::new()).expect("ядро запускается внутри рантайма");
    for line in [
        "register alice",
        "register bob",
        "follow bob alice",
        "post alice один",
        "post alice два",
        "post alice три",
    ] {
        execute_line(line, &net).await.expect("подготовка проходит");
    }
    let steps = [
        (
            "feed bob 2",
            "@alice [3] три (лайков: 0)\n@alice [2] два (лайков: 0)",
        ),
        (
            "page bob 1 2",
            "@alice [3] три (лайков: 0)\n@alice [2] два (лайков: 0)",
        ),
        ("page bob 2 2", "@alice [1] один (лайков: 0)"),
        ("page bob 3 2", "страницы 3 нет"),
        ("page bob 0", "страницы 0 нет"),
        (
            "page bob 1",
            "@alice [3] три (лайков: 0)\n@alice [2] два (лайков: 0)\n@alice [1] один (лайков: 0)",
        ),
        (
            "page bob 1 18446744073709551615",
            "@alice [3] три (лайков: 0)\n@alice [2] два (лайков: 0)\n@alice [1] один (лайков: 0)",
        ),
        (
            "page bob 18446744073709551615 2",
            "страницы 18446744073709551615 нет",
        ),
        ("delete alice 3", "пост [3] удалён"),
        ("delete alice 3", "пост [3] уже удалён"),
        (
            "page bob 1 2",
            "@alice [2] два (лайков: 0)\n@alice [1] один (лайков: 0)",
        ),
        ("mute bob alice", "@bob больше не видит @alice в ленте"),
        ("mute bob alice", "@bob уже скрыл @alice"),
        ("feed bob", "лента пуста"),
        ("page bob 1", "страницы 1 нет"),
    ];
    for (line, reply) in steps {
        assert_eq!(
            execute_line(line, &net).await,
            Ok(String::from(reply)),
            "ответ на команду {line:?}"
        );
    }
    assert_eq!(
        execute_line("delete bob 2", &net).await,
        Err(CommandError::Domain(DomainError::NotAuthor(PostId(2)))),
        "чужой пост: NotAuthor"
    );
    assert_eq!(
        execute_line("mute bob bob", &net).await,
        Err(CommandError::Domain(DomainError::SelfMute)),
        "себя не скрыть"
    );
    assert_eq!(
        execute_line("mute bob ghost", &net).await,
        Err(CommandError::UnknownName(String::from("ghost"))),
        "незнакомый ник"
    );
}
