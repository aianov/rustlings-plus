use std::error::Error;

use social_core::{
    DomainError, MAX_NAME_LEN, MAX_POST_LEN, MemNetwork, PostId, UserId, Visibility,
};

fn network_with(names: &[&str]) -> (MemNetwork, Vec<UserId>) {
    let mut net = MemNetwork::new();
    let mut ids = Vec::new();
    for name in names {
        ids.push(
            net.register(name)
                .expect("регистрация нового ника с нормальной длиной должна пройти"),
        );
    }
    (net, ids)
}

#[test]
fn empty_nick_is_rejected() {
    let mut net = MemNetwork::new();
    assert_eq!(
        net.register(""),
        Err(DomainError::EmptyName),
        "пустой ник: EmptyName"
    );
    assert_eq!(
        net.register("   "),
        Err(DomainError::EmptyName),
        "ник из одних пробелов после trim пустой: EmptyName"
    );
    assert_eq!(
        net.user_count(),
        0,
        "после ошибки пользователь не должен появиться"
    );
}

#[test]
fn long_nick_is_rejected() {
    let mut net = MemNetwork::new();
    let longest = "a".repeat(MAX_NAME_LEN);
    let too_long = "a".repeat(MAX_NAME_LEN + 1);
    assert!(
        net.register(&longest).is_ok(),
        "ник ровно из MAX_NAME_LEN символов разрешён"
    );
    assert_eq!(
        net.register(&too_long),
        Err(DomainError::NameTooLong),
        "ник длиннее MAX_NAME_LEN: NameTooLong"
    );
    assert!(
        net.register(&"я".repeat(MAX_NAME_LEN)).is_ok(),
        "длина ника считается в символах, а не в байтах: 20 букв «я» это 40 байт, но всего 20 символов"
    );
}

#[test]
fn taken_nick_is_rejected() {
    let (mut net, _) = network_with(&["alice"]);
    assert_eq!(
        net.register(" ALICE "),
        Err(DomainError::NameTaken(String::from("alice"))),
        "ник занят после нормализации: NameTaken с нормализованным ником внутри"
    );
    assert_eq!(net.user_count(), 1, "второй alice не должен появиться");
}

#[test]
fn cannot_follow_yourself() {
    let (mut net, ids) = network_with(&["alice"]);
    assert_eq!(
        net.follow(ids[0], ids[0]),
        Err(DomainError::SelfFollow),
        "подписка на себя: SelfFollow"
    );
    assert_eq!(
        net.following_count(ids[0]),
        0,
        "после ошибки подписка не должна появиться"
    );
}

#[test]
fn follow_needs_both_users() {
    let (mut net, ids) = network_with(&["alice"]);
    let ghost = UserId(99);
    assert_eq!(
        net.follow(ids[0], ghost),
        Err(DomainError::UnknownUser(ghost)),
        "подписка на несуществующего: UnknownUser"
    );
    assert_eq!(
        net.follow(ghost, ids[0]),
        Err(DomainError::UnknownUser(ghost)),
        "подписка от несуществующего: UnknownUser"
    );
    assert_eq!(
        net.followers_count(ids[0]),
        0,
        "после ошибки подписчик не должен появиться"
    );
}

#[test]
fn post_rules() {
    let (mut net, ids) = network_with(&["alice"]);
    let alice = ids[0];
    assert_eq!(
        net.publish(alice, "  ", Visibility::Public),
        Err(DomainError::EmptyPost),
        "пост из пробелов: EmptyPost"
    );
    let too_long = "ы".repeat(MAX_POST_LEN + 1);
    assert_eq!(
        net.publish(alice, &too_long, Visibility::Public),
        Err(DomainError::PostTooLong),
        "пост длиннее MAX_POST_LEN символов: PostTooLong"
    );
    assert!(
        net.publish(alice, &"ы".repeat(MAX_POST_LEN), Visibility::Public)
            .is_ok(),
        "пост ровно из MAX_POST_LEN символов разрешён, длина считается в символах"
    );
    assert_eq!(
        net.publish(UserId(99), "привет", Visibility::Public),
        Err(DomainError::UnknownUser(UserId(99))),
        "публиковать может только зарегистрированный пользователь"
    );
    assert_eq!(net.post_count(), 1, "посты с ошибкой не сохраняются");
}

#[test]
fn post_text_is_trimmed() {
    let (mut net, ids) = network_with(&["alice"]);
    let id = net
        .publish(ids[0], "  привет  ", Visibility::Public)
        .expect("непустой пост должен опубликоваться");
    let post = net.post(id).expect("опубликованный пост находится по id");
    assert_eq!(
        post.text(),
        "привет",
        "текст поста сохраняется без пробелов по краям"
    );
}

#[test]
fn like_needs_user_and_post() {
    let (mut net, ids) = network_with(&["alice", "bob"]);
    let post = net
        .publish(ids[0], "лайкните", Visibility::Public)
        .expect("непустой пост должен опубликоваться");
    assert_eq!(
        net.like(ids[1], PostId(999)),
        Err(DomainError::UnknownPost(PostId(999))),
        "лайк несуществующего поста: UnknownPost"
    );
    assert_eq!(
        net.like(UserId(99), post),
        Err(DomainError::UnknownUser(UserId(99))),
        "лайк от несуществующего пользователя: UnknownUser"
    );
    assert_eq!(net.likes(post), 0, "после ошибок лайков нет");
}

#[test]
fn lookups_return_option() {
    let (mut net, ids) = network_with(&["alice", "bob"]);
    let found = net
        .find_user("  ALICE ")
        .expect("find_user нормализует ник и находит alice");
    assert_eq!(found.id(), ids[0], "find_user должен найти именно alice");
    assert_eq!(found.name(), "alice", "ник хранится нормализованным");
    assert!(
        net.find_user("carol").is_none(),
        "carol не регистрировалась: None"
    );
    assert_eq!(
        net.user(ids[1]).map(|u| u.name()),
        Some("bob"),
        "user(id) находит bob"
    );
    assert!(
        net.user(UserId(99)).is_none(),
        "user(id) для незнакомого id: None"
    );
    assert!(
        net.post(PostId(1)).is_none(),
        "постов ещё нет: post(id) отдаёт None"
    );
    let id = net
        .publish(ids[0], "раз", Visibility::Public)
        .expect("непустой пост должен опубликоваться");
    assert_eq!(
        net.post(id).map(|p| p.text()),
        Some("раз"),
        "post(id) находит опубликованный пост"
    );
}

#[test]
fn last_post_of_author() {
    let (mut net, ids) = network_with(&["alice", "bob"]);
    assert!(
        net.last_post(ids[0]).is_none(),
        "у alice ещё нет постов: None"
    );
    for (author, text) in [(ids[0], "первый"), (ids[0], "второй"), (ids[1], "чужой")]
    {
        net.publish(author, text, Visibility::Public)
            .expect("непустой пост должен опубликоваться");
    }
    let last = net.last_post(ids[0]).expect("у alice два поста");
    assert_eq!(
        last.text(),
        "второй",
        "last_post отдаёт самый свежий пост автора, чужие не считаются"
    );
}

#[test]
fn errors_read_like_sentences() {
    let cases = [
        (DomainError::EmptyName, "ник не может быть пустым"),
        (DomainError::NameTooLong, "ник длиннее 20 символов"),
        (
            DomainError::NameTaken(String::from("alice")),
            "ник alice уже занят",
        ),
        (DomainError::SelfFollow, "нельзя подписаться на самого себя"),
        (
            DomainError::UnknownUser(UserId(7)),
            "нет пользователя с id 7",
        ),
        (DomainError::UnknownPost(PostId(7)), "нет поста с id 7"),
        (DomainError::EmptyPost, "пост не может быть пустым"),
        (DomainError::PostTooLong, "пост длиннее 280 символов"),
    ];
    for (error, text) in cases {
        assert_eq!(
            error.to_string(),
            text,
            "Display для {error:?} должен печатать ровно эту фразу"
        );
    }
}

#[test]
fn domain_error_is_a_std_error() {
    let boxed: Box<dyn Error> = Box::new(DomainError::SelfFollow);
    assert_eq!(
        boxed.to_string(),
        "нельзя подписаться на самого себя",
        "DomainError должен реализовать std::error::Error"
    );
}

#[test]
fn question_mark_carries_errors() -> Result<(), DomainError> {
    let mut net = MemNetwork::new();
    let alice = net.register("alice")?;
    let bob = net.register("bob")?;
    net.follow(bob, alice)?;
    let post = net.publish(alice, "с вопросиками удобнее #rust", Visibility::Followers)?;
    net.like(bob, post)?;
    assert_eq!(
        net.likes(post),
        1,
        "вся цепочка через ? должна пройти без ошибок"
    );
    Ok(())
}
