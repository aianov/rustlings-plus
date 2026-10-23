use std::collections::HashSet;

use contracts::{COMMAND_METHODS, COMMAND_NAMES, Command};

const SAMPLES: [&str; 18] = [
    "register alice",
    "post alice привет",
    "follow bob alice",
    "like bob 1",
    "feed bob",
    "inbox bob",
    "mute bob alice",
    "delete alice 1",
    "page bob 1",
    "posts bob alice",
    "comment bob 1 класс",
    "reply alice 1 спасибо",
    "thread bob 1",
    "trends",
    "stats",
    "import seed.txt",
    "set post_limit 100",
    "card bob alice",
];

#[test]
fn every_command_has_a_service_method() {
    assert_eq!(
        SAMPLES.map(|line| line.split_whitespace().next().unwrap_or_default()),
        COMMAND_NAMES[..18],
        "образцы покрывают первые восемнадцать текстовых команд по порядку, новые вехи дописывают свои в конец"
    );
    let expected = [
        "users.register",
        "posts.create",
        "users.follow",
        "posts.like",
        "posts.feed",
        "notify.inbox",
        "users.mute",
        "posts.delete",
        "posts.page",
        "posts.author",
        "comments.create",
        "comments.reply",
        "comments.thread",
        "posts.trends",
        "admin.stats",
        "admin.import",
        "admin.set",
        "users.card",
    ];
    for (line, method) in SAMPLES.iter().zip(expected) {
        let command: Command = line.parse().expect("образец разбирается");
        assert_eq!(command.method(), method, "метод команды {line:?}");
    }
}

#[test]
fn methods_are_unique_and_listed() {
    let unique: HashSet<&str> = COMMAND_METHODS.iter().copied().collect();
    assert_eq!(unique.len(), COMMAND_METHODS.len(), "методы не повторяются");
    for method in COMMAND_METHODS {
        let (service, name) = method
            .split_once('.')
            .unwrap_or_else(|| panic!("{method:?}: метод пишется как service.method"));
        assert!(
            !service.is_empty() && !name.is_empty() && !name.contains('.'),
            "{method:?}: одна точка между сервисом и методом"
        );
    }
    for line in SAMPLES {
        let command: Command = line.parse().expect("образец разбирается");
        assert!(
            COMMAND_METHODS.contains(&command.method()),
            "метод {} есть в COMMAND_METHODS",
            command.method()
        );
    }
    assert!(COMMAND_METHODS.contains(&"chat.send") && COMMAND_METHODS.contains(&"chat.history"));
}
