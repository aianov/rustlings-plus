use backend::{Post, PostId, PostKind, User, UserId, Visibility};

fn alice() -> UserId {
    UserId(1)
}

fn bob() -> UserId {
    UserId(2)
}

#[test]
fn ids_are_copied_and_compared_by_value() {
    let first = UserId(7);
    let copy = first;
    assert_eq!(
        first, copy,
        "UserId должен копироваться (derive Clone, Copy) и сравниваться (derive PartialEq)"
    );
    assert_ne!(UserId(7), UserId(8), "UserId с разными числами внутри не равны");
    assert_eq!(PostId(3), PostId(3), "PostId сравнивается по числу внутри");
}

#[test]
fn new_user_has_name_and_empty_bio() {
    let user = User::new(UserId(1), String::from("alice"));
    assert_eq!(user.id(), UserId(1), "User::new должен запомнить id");
    assert_eq!(user.name(), "alice", "User::new должен запомнить ник");
    assert_eq!(user.bio(), "", "у нового пользователя био пустое");
}

#[test]
fn user_can_be_renamed() {
    let mut user = User::new(UserId(1), String::from("alice"));
    user.rename(String::from("alice_rs"));
    assert_eq!(user.name(), "alice_rs", "rename должен заменить ник");
    assert_eq!(user.id(), UserId(1), "при переименовании id не меняется");
}

#[test]
fn user_can_change_bio() {
    let mut user = User::new(UserId(1), String::from("alice"));
    user.set_bio(String::from("Пишу на Rust"));
    assert_eq!(user.bio(), "Пишу на Rust", "set_bio должен записать био");
    user.set_bio(String::new());
    assert_eq!(user.bio(), "", "set_bio с пустой строкой стирает био");
    assert_eq!(user.name(), "alice", "set_bio не трогает ник");
}

#[test]
fn new_post_is_text_without_likes() {
    let post = Post::new(PostId(10), alice(), String::from("Привет, мир!"), Visibility::Public);
    assert_eq!(post.id(), PostId(10), "Post::new должен запомнить id");
    assert_eq!(post.author(), alice(), "Post::new должен запомнить автора");
    assert_eq!(post.text(), "Привет, мир!", "Post::new должен запомнить текст");
    assert_eq!(post.kind(), PostKind::Text, "Post::new создаёт обычный текстовый пост");
    assert_eq!(post.visibility(), Visibility::Public, "Post::new должен запомнить видимость");
    assert_eq!(post.likes(), 0, "у нового поста ноль лайков");
    assert!(!post.is_repost(), "текстовый пост не репост");
}

#[test]
fn likes_add_up() {
    let mut post = Post::new(PostId(10), alice(), String::from("лайкните меня"), Visibility::Public);
    post.like(UserId(2));
    post.like(UserId(3));
    post.like(UserId(4));
    assert_eq!(post.likes(), 3, "три лайка от трёх разных людей дают три лайка");
}

#[test]
fn repost_points_to_original() {
    let repost = Post::repost(PostId(11), bob(), PostId(10));
    assert_eq!(repost.id(), PostId(11), "у репоста свой id");
    assert_eq!(repost.kind(), PostKind::Repost(PostId(10)), "репост помнит id исходного поста");
    assert!(repost.is_repost(), "is_repost у репоста должен быть true");
    assert_eq!(repost.author(), bob(), "автор репоста тот, кто репостнул");
    assert_eq!(repost.text(), "", "у репоста нет своего текста");
    assert_eq!(repost.visibility(), Visibility::Public, "репост виден всем");
    assert_eq!(repost.likes(), 0, "у нового репоста ноль лайков");
}

#[test]
fn public_post_is_visible_to_everyone() {
    let post = Post::new(PostId(1), alice(), String::from("всем привет"), Visibility::Public);
    assert!(post.can_view(alice(), false), "автор видит свой пост");
    assert!(post.can_view(bob(), false), "публичный пост видит даже тот, кто не подписан");
    assert!(post.can_view(bob(), true), "публичный пост видит подписчик");
}

#[test]
fn followers_post_needs_a_follow() {
    let post = Post::new(PostId(2), alice(), String::from("только для своих"), Visibility::Followers);
    assert!(post.can_view(alice(), false), "автор всегда видит свой пост");
    assert!(post.can_view(bob(), true), "подписчик видит пост для подписчиков");
    assert!(!post.can_view(bob(), false), "без подписки пост для подписчиков не виден");
}

#[test]
fn private_post_is_for_author_only() {
    let post = Post::new(PostId(3), alice(), String::from("дневник"), Visibility::Private);
    assert!(post.can_view(alice(), false), "автор видит свой личный пост");
    assert!(!post.can_view(bob(), true), "личный пост не видит никто, кроме автора, даже подписчик");
    assert!(!post.can_view(bob(), false), "личный пост не виден чужим");
}
