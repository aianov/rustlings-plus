use backend::{MemNetwork, Post, PostId, UserId, Visibility};

struct World {
    net: MemNetwork,
    alice: UserId,
    bob: UserId,
    carol: UserId,
    dave: UserId,
}

fn world() -> World {
    let mut net = MemNetwork::new();
    let mut users = Vec::new();
    for name in ["alice", "bob", "carol", "dave"] {
        users.push(net.register(name).expect("регистрация нового ника должна пройти"));
    }
    let (alice, bob, carol, dave) = (users[0], users[1], users[2], users[3]);
    net.follow(alice, bob).expect("alice подписывается на bob");
    net.follow(alice, carol).expect("alice подписывается на carol");
    World { net, alice, bob, carol, dave }
}

fn publish(w: &mut World, author: UserId, text: &str, visibility: Visibility) -> PostId {
    w.net
        .publish(author, text, visibility)
        .expect("публикация непустого поста должна пройти")
}

fn ids(posts: &[&Post]) -> Vec<PostId> {
    posts.iter().map(|post| post.id()).collect()
}

struct Timeline {
    b1: PostId,
    c1: PostId,
    d1: PostId,
    b2: PostId,
    a1: PostId,
    c2: PostId,
    a2: PostId,
}

fn timeline(w: &mut World) -> Timeline {
    let (alice, bob, carol, dave) = (w.alice, w.bob, w.carol, w.dave);
    Timeline {
        b1: publish(w, bob, "bob всем #rust", Visibility::Public),
        c1: publish(w, carol, "carol своим #rust", Visibility::Followers),
        d1: publish(w, dave, "dave всем #rust", Visibility::Public),
        b2: publish(w, bob, "bob себе #rust", Visibility::Private),
        a1: publish(w, alice, "alice всем", Visibility::Public),
        c2: publish(w, carol, "carol всем", Visibility::Public),
        a2: publish(w, alice, "alice себе", Visibility::Private),
    }
}

#[test]
fn feed_is_follows_and_own_newest_first() {
    let mut w = world();
    let t = timeline(&mut w);
    assert_eq!(
        ids(&w.net.feed(w.alice, 10)),
        vec![t.a2, t.c2, t.a1, t.c1, t.b1],
        "лента alice: свои посты и посты bob и carol, новые сверху; dave она не читает, личный пост bob ей не виден"
    );
}

#[test]
fn feed_is_limited() {
    let mut w = world();
    let t = timeline(&mut w);
    assert_eq!(ids(&w.net.feed(w.alice, 2)), vec![t.a2, t.c2], "feed(viewer, 2) отдаёт два самых свежих поста");
    assert!(w.net.feed(w.alice, 0).is_empty(), "feed(viewer, 0) пустая");
}

#[test]
fn feed_respects_visibility() {
    let mut w = world();
    let t = timeline(&mut w);
    assert_eq!(ids(&w.net.feed(w.bob, 10)), vec![t.b2, t.b1], "bob ни на кого не подписан: в ленте только свои посты, включая личный");
    assert_eq!(ids(&w.net.feed(w.dave, 10)), vec![t.d1], "у dave в ленте только его пост");
    let carol_feed = ids(&w.net.feed(w.carol, 10));
    assert_eq!(carol_feed, vec![t.c2, t.c1], "carol ни на кого не подписана: только свои посты");
}

#[test]
fn feed_of_unknown_viewer_is_empty() {
    let mut w = world();
    timeline(&mut w);
    assert!(w.net.feed(UserId(99), 10).is_empty(), "у незнакомого зрителя лента пустая");
}

#[test]
fn feed_points_into_the_store() {
    let mut w = world();
    let t = timeline(&mut w);
    let feed = w.net.feed(w.alice, 1);
    let stored = w.net.post(t.a2).expect("пост лежит в хранилище");
    assert!(
        std::ptr::eq(feed[0], stored),
        "лента должна отдавать ссылки на посты из хранилища, а не копии"
    );
    let other = w.net.feed(w.bob, 10);
    assert_eq!(feed.len() + other.len(), 3, "две ленты можно держать одновременно: feed берёт &self");
}

#[test]
fn search_by_tag_shows_only_visible_posts() {
    let mut w = world();
    let t = timeline(&mut w);
    assert_eq!(
        ids(&w.net.search_tag(w.alice, "#Rust")),
        vec![t.d1, t.c1, t.b1],
        "поиск alice по #rust: все видимые ей посты с тегом, новые сверху; личного поста bob нет"
    );
    assert_eq!(ids(&w.net.search_tag(w.dave, "rust")), vec![t.d1, t.b1], "dave не подписан на carol: её пост для подписчиков ему не виден");
    assert_eq!(ids(&w.net.search_tag(w.bob, "rust")), vec![t.b2, t.d1, t.b1], "bob видит свой личный пост в поиске");
    assert!(w.net.search_tag(w.alice, "go").is_empty(), "постов с #go нет");
}

#[test]
fn counters_by_author() {
    let mut w = world();
    let t = timeline(&mut w);
    let (alice, bob, carol, dave) = (w.alice, w.bob, w.carol, w.dave);
    for (who, post) in [(alice, t.b1), (carol, t.b1), (dave, t.b1), (bob, t.b2), (bob, t.c2)] {
        w.net.like(who, post).expect("лайк существующего поста проходит");
    }
    assert_eq!(w.net.post_count_of(alice), 2, "у alice два поста");
    assert_eq!(w.net.post_count_of(bob), 2, "у bob два поста");
    assert_eq!(w.net.post_count_of(UserId(99)), 0, "у незнакомца постов нет");
    assert_eq!(w.net.likes_received(bob), 4, "bob получил 3 лайка на первый пост и 1 (свой) на второй");
    assert_eq!(w.net.likes_received(carol), 1, "carol получила один лайк");
    assert_eq!(w.net.likes_received(dave), 0, "dave лайков не получал");
}
