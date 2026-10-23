use repl::Mode;
use social_core::Notifiers;

fn session(input: &str) -> String {
    let mut out = Vec::new();
    repl::run(Mode::Text, Notifiers::new(), input.as_bytes(), &mut out)
        .expect("сеанс терминала проходит без ошибок ввода и вывода");
    String::from_utf8(out).expect("терминал пишет UTF-8")
}

#[test]
fn tasks_command_dumps_the_console() {
    let out = session("register alice\ntasks\n");
    let lines: Vec<&str> = out.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.starts_with("задач живых "))
        .unwrap_or_else(|| panic!("команда tasks печатает строку счётчиков:\n{out}"));
    assert_eq!(
        lines[at], "задач живых 10, запущено 10, завершено 0, брошено 0, паник 0",
        "у свежего ядра десять фоновых задач, и все живы"
    );
    let tasks: Vec<&str> = lines[at + 1..]
        .iter()
        .copied()
        .filter(|line| line.starts_with('#'))
        .collect();
    assert_eq!(tasks.len(), 10, "по строке на живую задачу:\n{out}");
    for name in [
        "trends-ticker",
        "trends-actor",
        "fanout-supervisor",
        "fanout-worker",
    ] {
        assert!(
            tasks
                .iter()
                .any(|line| line.contains(&format!(" {name}: "))),
            "в дампе есть задача {name}:\n{out}"
        );
    }
}
