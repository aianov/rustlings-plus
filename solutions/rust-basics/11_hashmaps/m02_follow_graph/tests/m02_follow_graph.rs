use backend::{Network, Post, PostId, UserId, Visibility, normalize_name};

#[test]
fn nick_is_trimmed_and_lowercased() {
    assert_eq!(normalize_name("  Alice "), "alice", "ник обрезается по краям и приводится к нижнему регистру");
    assert_eq!(normalize_name("BOB"), "bob", "заглавные буквы в нике становятся строчными");
    assert_eq!(normalize_name("carol"), "carol", "нормальный ник не меняется");
}

#[test]
fn register_gives_each_user_own_id() {
    let mut net = Network::new();
    let alice = net.register("alice");
    let bob = net.register("bob");
    assert_ne!(alice, bob, "у разных пользователей разные id");
    assert_eq!(net.user_count(), 2, "после двух регистраций в сети два пользователя");
}

#[test]
fn same_nick_means_same_user() {
    let mut net = Network::new();
    let first = net.register("Alice");
    let second = net.register("  alice  ");
    assert_eq!(first, second, "\"Alice\" и \"  alice  \" после нормализации один ник: register отдаёт тот же id");
    assert_eq!(net.user_count(), 1, "повторная регистрация ника не создаёт второго пользователя");
    assert!(net.is_registered("ALICE"), "is_registered тоже нормализует ник");
    assert!(!net.is_registered("bob"), "bob не регистрировался");
}

#[test]
fn follow_counts_both_sides() {
    let mut net = Network::new();
    let alice = net.register("alice");
    let bob = net.register("bob");
    let carol = net.register("carol");
    assert!(net.follow(alice, bob), "первая подписка alice на bob должна вернуть true");
    assert!(net.follow(carol, bob), "первая подписка carol на bob должна вернуть true");
    assert!(net.is_following(alice, bob), "alice подписана на bob");
    assert!(!net.is_following(bob, alice), "подписка не взаимная: bob на alice не подписан");
    assert_eq!(net.following_count(alice), 1, "alice подписана на одного человека");
    assert_eq!(net.followers_count(bob), 2, "у bob два подписчика");
    assert_eq!(net.followers_count(alice), 0, "у alice подписчиков нет");
    assert_eq!(net.following_count(bob), 0, "bob ни на кого не подписан");
}

#[test]
fn follow_twice_does_not_duplicate() {
    let mut net = Network::new();
    let alice = net.register("alice");
    let bob = net.register("bob");
    assert!(net.follow(alice, bob), "первая подписка должна вернуть true");
    assert!(!net.follow(alice, bob), "повторная подписка ничего не меняет и возвращает false");
    assert_eq!(net.following_count(alice), 1, "повторная подписка не должна дублироваться в подписках");
    assert_eq!(net.followers_count(bob), 1, "повторная подписка не должна дублироваться в подписчиках");
}

#[test]
fn cannot_follow_yourself() {
    let mut net = Network::new();
    let alice = net.register("alice");
    assert!(!net.follow(alice, alice), "подписка на себя ничего не делает и возвращает false");
    assert_eq!(net.following_count(alice), 0, "подписка на себя не считается");
    assert_eq!(net.followers_count(alice), 0, "подписка на себя не считается");
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
    let mut net = Network::new();
    let alice = net.register("alice");
    let bob = net.register("bob");
    let carol = net.register("carol");
    let post = net.publish(alice, "Мой первый пост", Visibility::Public);
    assert!(net.like(bob, post), "первый лайк bob засчитан");
    assert!(!net.like(bob, post), "повторный лайк bob не засчитан");
    assert!(net.like(carol, post), "лайк carol засчитан");
    assert_eq!(net.likes(post), 2, "у поста два лайка: от bob и от carol");
}

#[test]
fn like_of_unknown_post_is_ignored() {
    let mut net = Network::new();
    let bob = net.register("bob");
    assert!(!net.like(bob, PostId(999)), "лайк несуществующего поста возвращает false");
    assert_eq!(net.likes(PostId(999)), 0, "у несуществующего поста ноль лайков");
}

#[test]
fn publish_gives_each_post_own_id() {
    let mut net = Network::new();
    let alice = net.register("alice");
    let first = net.publish(alice, "раз", Visibility::Public);
    let second = net.publish(alice, "два", Visibility::Private);
    assert_ne!(first, second, "у разных постов разные id");
    assert_eq!(net.post_count(), 2, "после двух публикаций в сети два поста");
}

#[test]
fn hashtags_are_indexed() {
    let mut net = Network::new();
    let alice = net.register("alice");
    let first = net.publish(alice, "Учу #Rust и #rust, а ещё #tokio!", Visibility::Public);
    let plain = net.publish(alice, "Без тегов, только # и решётка", Visibility::Public);
    let third = net.publish(alice, "#rust снова", Visibility::Public);
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
