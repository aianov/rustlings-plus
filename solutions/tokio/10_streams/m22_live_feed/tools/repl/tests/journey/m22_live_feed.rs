use contracts::{ChatCommand, Username};
use repl::{Action, Mode, Session, SessionError};
use social_core::Notifiers;

fn name(nick: &str) -> Username {
    Username::try_from(nick).expect("ник из теста корректный")
}

fn session(input: &str) -> String {
    let mut out = Vec::new();
    repl::run(Mode::Text, Notifiers::new(), input.as_bytes(), &mut out)
        .expect("сеанс терминала проходит без ошибок ввода и вывода");
    String::from_utf8(out).expect("терминал пишет UTF-8")
}

fn after<'a>(lines: &[&'a str], line: &str) -> Option<&'a str> {
    let at = lines
        .iter()
        .position(|candidate| *candidate == line)
        .unwrap_or_else(|| {
            panic!(
                "в выводе терминала нет строки {line:?}:\n{}",
                lines.join("\n")
            )
        });
    lines.get(at + 1).copied()
}

#[test]
fn watch_needs_a_session() {
    assert_eq!(
        Session::Anonymous.step(ChatCommand::Watch { recent: 3 }),
        Err(SessionError::NotLoggedIn),
        "следить можно только за своей лентой, значит сначала /login"
    );
    assert_eq!(
        Session::Anonymous.step(ChatCommand::Unwatch),
        Err(SessionError::NotLoggedIn),
        "без сессии и /unwatch отказ"
    );
    let bob = Session::LoggedIn { user: name("bob") };
    assert_eq!(
        bob.step(ChatCommand::Watch { recent: 3 }),
        Ok((
            bob.clone(),
            Action::Watch {
                user: name("bob"),
                recent: 3
            }
        )),
        "/watch не меняет сессию и просит терминал следить за лентой того, кто вошёл"
    );
    let chatting = Session::Chatting {
        user: name("bob"),
        peer: name("alice"),
    };
    assert_eq!(
        chatting.step(ChatCommand::Unwatch),
        Ok((chatting.clone(), Action::Unwatch)),
        "/unwatch тоже не трогает диалог"
    );
}

#[test]
fn terminal_shows_new_posts_live() {
    let out = session(
        "register alice\nregister bob\nregister carol\nfollow bob alice\npost alice раз\n\
         post alice два\n/login bob\n/watch 1\npost alice три\npost carol мимо\npost bob своё\n\
         /unwatch\npost alice четыре\n",
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        after(&lines, "@alice [2] два (лайков: 0)"),
        Some("слежу за лентой @bob, новые посты появятся здесь"),
        "/watch 1 печатает один последний пост ленты и строку о слежке"
    );
    assert!(
        !out.contains("@alice [1] раз"),
        "/watch 1 показывает только один последний пост:\n{out}"
    );
    assert_eq!(
        after(&lines, "опубликован пост [3]"),
        Some("вживую: @alice [3] три (лайков: 0)"),
        "новый пост подписки печатается сразу после ответа на команду, которая его создала"
    );
    assert_eq!(
        after(&lines, "опубликован пост [4]"),
        Some("опубликован пост [5]"),
        "пост carol не из ленты bob, живой строки для него нет"
    );
    assert_eq!(
        after(&lines, "опубликован пост [5]"),
        Some("вживую: @bob [5] своё (лайков: 0)"),
        "свой пост тоже в ленте"
    );
    assert_eq!(
        after(&lines, "больше не слежу за лентой @bob"),
        Some("опубликован пост [6]"),
        "/unwatch бросает стрим"
    );
    assert_eq!(
        lines.last().copied(),
        Some("опубликован пост [6]"),
        "после /unwatch живых строк нет"
    );
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("вживую: "))
            .count(),
        2,
        "живых строк ровно две:\n{out}"
    );
}

#[test]
fn logout_stops_the_watch() {
    let out = session(
        "register alice\nregister bob\nfollow bob alice\n/login bob\n/watch\n/logout\n\
         post alice после выхода\n/login bob\n/unwatch\n",
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        after(&lines, "лента пуста"),
        Some("слежу за лентой @bob, новые посты появятся здесь"),
        "пустая лента так и говорит, а слежка всё равно начинается"
    );
    assert!(
        !out.contains("вживую: "),
        "после /logout терминал бросает живую ленту:\n{out}"
    );
    assert_eq!(
        lines.last().copied(),
        Some("за лентой никто не следит"),
        "/unwatch без слежки отвечает спокойно, без ошибки"
    );
}
