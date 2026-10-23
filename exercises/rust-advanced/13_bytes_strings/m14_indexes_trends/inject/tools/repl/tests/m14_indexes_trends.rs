use repl::Mode;
use social_core::Notifiers;

fn session(script: &str) -> String {
    let mut out = Vec::new();
    repl::run(Mode::Text, Notifiers::new(), script.as_bytes(), &mut out)
        .expect("текстовый сеанс завершается Ok");
    String::from_utf8(out).expect("терминал пишет UTF-8")
}

#[test]
fn comments_and_trends_from_the_terminal() {
    let script = "register alice\nregister bob\nfollow bob alice\npost alice Что почитать про #rust?\ncomment bob 1 Растбук\nreply alice 1 Спасибо!\nreply bob 2 Пожалуйста\nlike bob 1\nthread bob 1\ntrends 5\n";
    let lines: Vec<String> = session(script).lines().map(String::from).collect();
    assert_eq!(
        lines[4..],
        [
            "комментарий [1] к посту [1]",
            "ответ [2] на комментарий [1]",
            "ответ [3] на комментарий [2]",
            "@bob лайкнул пост [1]",
            "[1] @bob: Растбук",
            "  [2] @alice: Спасибо!",
            "    [3] @bob: Пожалуйста",
            "@alice [1] Что почитать про #rust? (лайков: 1), за сутки +1",
        ]
        .map(String::from),
        "терминал знает comment, reply, thread и trends: команды идут через шину ядра"
    );
}

#[test]
fn author_pages_and_previews_from_the_terminal() {
    let long = "ы".repeat(100);
    let script = format!(
        "register alice\nregister bob\nfollow bob alice\npost alice раз\npost alice {long}\nposts bob alice 1\nfeed bob 1\n"
    );
    let out = session(&script);
    let lines: Vec<&str> = out.lines().collect();
    let preview = format!("@alice [2] {}… (лайков: 0)", "ы".repeat(40));
    assert_eq!(
        lines.len(),
        8,
        "на каждую команду ответ, а posts дал ещё строку курсора: {out}"
    );
    assert_eq!(
        lines[5], preview,
        "страница автора показывает превью длинного поста: 80 байт это 40 букв «ы»"
    );
    assert!(
        lines[6].starts_with("дальше: ") && lines[6].ends_with(":2"),
        "за неполным списком идёт курсор следующей страницы <время>:<id>, а пришло {:?}",
        lines[6]
    );
    assert_eq!(lines[7], preview, "лента тоже показывает превью");
}

#[test]
fn foreign_nick_is_explained() {
    assert_eq!(
        session("register алиса\nregister Alice\n"),
        "ошибка: в нике можно только a-z, 0-9 и _, а 'а' нельзя\nзарегистрирован @alice (id 1)\n",
        "ник не из a-z, 0-9 и _ отвергается с понятной причиной"
    );
}
