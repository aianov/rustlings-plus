use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use repl::{Mode, Terminal};
use social_core::Notifiers;

fn session(script: &str) -> String {
    let script = script.to_string();
    let (done, result) = mpsc::channel();
    thread::Builder::new()
        .spawn(move || {
            let mut out = Vec::new();
            let finished = repl::run(Mode::Text, Notifiers::new(), script.as_bytes(), &mut out);
            let _ = done.send(finished.map(|()| out));
        })
        .expect("ОС должна дать тесту поток");
    let out = match result.recv_timeout(Duration::from_secs(60)) {
        Ok(out) => out.expect("сеанс завершается Ok"),
        Err(RecvTimeoutError::Timeout) => {
            panic!("сеанс не закончился за 60 с: похоже, терминал ждёт ядро вечно")
        }
        Err(RecvTimeoutError::Disconnected) => panic!("сеанс упал, причина в сообщении выше"),
    };
    String::from_utf8(out).expect("терминал пишет UTF-8")
}

#[test]
fn admin_sets_the_post_limit_live() {
    let out = session(
        "register alice\nset post_limit 5\npost alice слишком длинно\npost alice ура!!\nset post_limit 280\npost alice слишком длинно\nset color 1\n",
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        [
            "зарегистрирован @alice (id 1)",
            "post_limit = 5",
            "ошибка: пост длиннее 5 символов: такой предел сейчас поставил админ",
            "опубликован пост [1]",
            "post_limit = 280",
            "опубликован пост [2]",
            "ошибка: неизвестная настройка \"color\", есть post_limit, trends_window",
        ],
        "set меняет предел сразу, без перезапуска терминала"
    );
}

#[test]
fn card_shows_the_profile_in_the_terminal() {
    let out = session(
        "register alice\nregister bob\nfollow bob alice\npost alice раз\npost alice два\npost alice три\npost alice четыре\nlike bob 4\ncard bob alice\n",
    );
    let card: Vec<&str> = out.lines().skip(8).collect();
    assert_eq!(
        card,
        [
            "@alice",
            "подписчиков 1, подписок 0, постов 4, лайков 1",
            "ты подписан",
            "[4] четыре (лайков: 1)",
            "[3] три (лайков: 0)",
            "[2] два (лайков: 0)",
        ],
        "карточка: ник, счётчики, подписка и три последних поста"
    );
}

#[test]
fn terminal_answers_card_through_the_bus() {
    let mut terminal = Terminal::new().expect("терминал запускается");
    for line in ["register alice", "register bob"] {
        assert!(terminal.handle(line).is_ok(), "{line} проходит");
    }
    let reply = terminal.handle("card bob ghost");
    assert_eq!(
        (reply.status, reply.body.as_str()),
        (404, "нет пользователя @ghost"),
        "карточка незнакомца: 404 из шины"
    );
}
