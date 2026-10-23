use social_core::{MemNetwork, Post, PostId, UserId, Visibility, normalize_name};

fn user(net: &mut MemNetwork, name: &str) -> UserId {
    net.register(name)
        .expect("регистрация нового ника с нормальной длиной должна пройти")
}

fn publish(net: &mut MemNetwork, author: UserId, text: &str) -> PostId {
    net.publish(author, text, Visibility::Public)
        .expect("публикация непустого поста от существующего автора должна пройти")
}

#[test]
fn nick_is_trimmed_and_lowercased() {
    assert_eq!(normalize_name("  Alice "), "alice", "ник обрезается по краям и приводится к нижнему регистру");
    assert_eq!(normalize_name("BOB"), "bob", "заглавные буквы в нике становятся строчными");
    assert_eq!(normalize_name("carol"), "carol", "нормальный ник не меняется");
}

#[test]
fn register_gives_each_user_own_id() {
    let mut net = MemNetwork::new();
    let alice = user(&mut net, "alice");
    let bob = user(&mut net, "bob");
    assert_ne!(alice, bob, "у разных пользователей разные id");
    assert_eq!(net.user_count(), 2, "после двух регистраций в сети два пользователя");
}

#[test]
fn registered_nick_is_normalized() {
    let mut net = MemNetwork::new();
    user(&mut net, "  Alice ");
    assert!(net.is_registered("ALICE"), "is_registered нормализует ник");
    assert!(net.is_registered("alice"), "ник хранится нормализованным");
    assert!(!net.is_registered("bob"), "bob не регистрировался");
}

#[test]
fn follow_counts_both_sides() {
    let mut net = MemNetwork::new();
    let alice = user(&mut net, "alice");
    let bob = user(&mut net, "bob");
    let carol = user(&mut net, "carol");
    assert_eq!(net.follow(alice, bob), Ok(true), "первая подписка alice на bob должна вернуть Ok(true)");
    assert_eq!(net.follow(carol, bob), Ok(true), "первая подписка carol на bob должна вернуть Ok(true)");
    assert!(net.is_following(alice, bob), "alice подписана на bob");
    assert!(!net.is_following(bob, alice), "подписка не взаимная: bob на alice не подписан");
    assert_eq!(net.following_count(alice), 1, "alice подписана на одного человека");
    assert_eq!(net.followers_count(bob), 2, "у bob два подписчика");
    assert_eq!(net.followers_count(alice), 0, "у alice подписчиков нет");
    assert_eq!(net.following_count(bob), 0, "bob ни на кого не подписан");
}

#[test]
fn follow_twice_does_not_duplicate() {
    let mut net = MemNetwork::new();
    let alice = user(&mut net, "alice");
    let bob = user(&mut net, "bob");
    assert_eq!(net.follow(alice, bob), Ok(true), "первая подписка должна вернуть Ok(true)");
    assert_eq!(net.follow(alice, bob), Ok(false), "повторная подписка ничего не меняет и возвращает Ok(false)");
    assert_eq!(net.following_count(alice), 1, "повторная подписка не должна дублироваться в подписках");
    assert_eq!(net.followers_count(bob), 1, "повторная подписка не должна дублироваться в подписчиках");
}

#[test]
fn post_remembers_who_liked_it() {
    let mut post = Post::new(PostId(1), UserId(1), String::from("лайкни меня"), Visibility::Public);
    assert!(post.like(UserId(2)), "первый лайк от пользователя возвращает true");
    assert!(!post.like(UserId(2)), "второй лайк от того же пользователя возвращает false");
    assert!(post.is_liked_by(UserId(2)), "пост помнит, кто его лайкнул");
    assert!(!post.is_liked_by(UserId(3)), "пользователь 3 пост не лайкал");
    assert_eq!(post.likes(), 1, "один человек даёт один лайк, сколько бы раз ни нажал");
}

#[test]
fn like_through_network_counts_each_user_once() {
    let mut net = MemNetwork::new();
    let alice = user(&mut net, "alice");
    let bob = user(&mut net, "bob");
    let carol = user(&mut net, "carol");
    let post = publish(&mut net, alice, "Мой первый пост");
    assert_eq!(net.like(bob, post), Ok(true), "первый лайк bob засчитан");
    assert_eq!(net.like(bob, post), Ok(false), "повторный лайк bob не засчитан");
    assert_eq!(net.like(carol, post), Ok(true), "лайк carol засчитан");
    assert_eq!(net.likes(post), 2, "у поста два лайка: от bob и от carol");
}

#[test]
fn publish_gives_each_post_own_id() {
    let mut net = MemNetwork::new();
    let alice = user(&mut net, "alice");
    let first = publish(&mut net, alice, "раз");
    let second = publish(&mut net, alice, "два");
    assert_ne!(first, second, "у разных постов разные id");
    assert_eq!(net.post_count(), 2, "после двух публикаций в сети два поста");
}

#[test]
fn hashtags_are_indexed() {
    let mut net = MemNetwork::new();
    let alice = user(&mut net, "alice");
    let first = publish(&mut net, alice, "Учу #Rust и #rust, а ещё #tokio!");
    let plain = publish(&mut net, alice, "Без тегов, только # и решётка");
    let third = publish(&mut net, alice, "#rust снова");
    assert_eq!(
        net.posts_with_tag("rust"),
        vec![first, third],
        "тег #rust есть в первом и третьем посте; повтор #Rust и #rust в одном посте считается один раз"
    );
    assert_eq!(net.posts_with_tag("#RUST"), vec![first, third], "поиск по тегу не зависит от регистра и решётки");
    assert_eq!(net.posts_with_tag("tokio"), vec![first], "у #tokio! восклицательный знак не входит в тег");
    assert!(net.posts_with_tag("go").is_empty(), "постов с тегом #go нет");
    assert!(!net.posts_with_tag("rust").contains(&plain), "одинокая # не делает тег");
}
