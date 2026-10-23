use repl::Terminal;
use social_core::Response;

#[test]
fn terminal_knows_mute_delete_and_page() {
    let mut terminal = Terminal::new().expect("терминал должен запускаться");
    let script = [
        ("register alice", 200, "зарегистрирован @alice (id 1)"),
        ("register bob", 200, "зарегистрирован @bob (id 2)"),
        ("follow bob alice", 200, "@bob подписан на @alice"),
        ("post alice раз", 200, "опубликован пост [1]"),
        ("post alice два", 200, "опубликован пост [2]"),
        ("page bob 2 1", 200, "@alice [1] раз (лайков: 0)"),
        ("page bob 3 1", 200, "страницы 3 нет"),
        ("delete bob 1", 422, "удалить пост [1] может только автор"),
        ("delete alice 1", 200, "пост [1] удалён"),
        ("page bob 1", 200, "@alice [2] два (лайков: 0)"),
        ("like bob 1", 422, "нет поста с id 1"),
        ("mute bob alice", 200, "@bob больше не видит @alice в ленте"),
        ("feed bob", 200, "лента пуста"),
        ("page bob", 400, "не хватает аргумента: номер страницы"),
    ];
    for (line, status, body) in script {
        assert_eq!(
            terminal.handle(line),
            Response::new(status, body),
            "ответ терминала на {line:?}: новые команды пришли в шину из COMMAND_NAMES"
        );
    }
}
