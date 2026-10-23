use contracts::{COMMAND_NAMES, Command, ParseError, PostId, Push, UserId};
use serde_json::json;

#[test]
fn new_post_push_on_the_wire() {
    let push = Push::NewPost {
        to: UserId(2),
        author: UserId(1),
        post: PostId(7),
    };
    let value = serde_json::to_value(push).expect("push сериализуется");
    assert_eq!(
        value,
        json!({"push": "new_post", "to": "u_2", "author": "u_1", "post": "p_7"}),
        "push о новом посте: тег new_post, получатель в to, автор и пост строками id"
    );
    assert_eq!(
        serde_json::from_value::<Push>(value).ok(),
        Some(push),
        "push о новом посте читается обратно"
    );
}

#[test]
fn stats_command_parses() {
    assert_eq!(
        "stats".parse::<Command>(),
        Ok(Command::Stats),
        "команда stats без аргументов"
    );
    assert_eq!(
        "  stats  ".parse::<Command>(),
        Ok(Command::Stats),
        "пробелы по краям не мешают"
    );
    assert_eq!(
        "stats alice".parse::<Command>(),
        Err(ParseError::ExtraArgument(String::from("alice"))),
        "у stats нет аргументов, лишний это ExtraArgument"
    );
}

#[test]
fn stats_is_listed_last() {
    assert_eq!(
        COMMAND_NAMES,
        [
            "register", "post", "follow", "like", "feed", "inbox", "mute", "delete", "page",
            "posts", "comment", "reply", "thread", "trends", "stats"
        ],
        "COMMAND_NAMES дописан командой stats в конец, тогда шина ядра её зарегистрирует"
    );
}
