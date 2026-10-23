use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn terminal_talks_through_stdin_and_stdout() {
    let script = "register alice\nregister bob\n\npost alice Первый пост #hello\nfollow bob alice\nlike bob 1\nfeed bob\ninbox alice\ndance\nfollow bob ghost\n";
    let expected = "зарегистрирован @alice (id 1)\n\
                    зарегистрирован @bob (id 2)\n\
                    опубликован пост [1]\n\
                    @bob подписан на @alice\n\
                    @bob лайкнул пост [1]\n\
                    @alice [1] Первый пост #hello (лайков: 1)\n\
                    @bob подписался на тебя\n\
                    @bob лайкнул твой пост [1]\n\
                    ошибка: неизвестная команда: dance\n\
                    ошибка: нет пользователя @ghost\n";
    let mut child = Command::new(env!("CARGO_BIN_EXE_repl"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("бинарник repl (tools/repl) должен запускаться");
    child
        .stdin
        .take()
        .expect("у процесса есть stdin")
        .write_all(script.as_bytes())
        .expect("скрипт пишется в stdin");
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait().expect("состояние процесса читается") {
            Some(status) => break status,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                panic!("терминал не завершился за 10 секунд после конца stdin: цикл по строкам должен кончаться на EOF");
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    let mut output = String::new();
    child
        .stdout
        .take()
        .expect("у процесса есть stdout")
        .read_to_string(&mut output)
        .expect("stdout читается");
    assert!(status.success(), "терминал должен завершаться успешно, когда stdin кончился");
    assert_eq!(output, expected, "терминал отвечает строкой на каждую непустую команду, ошибки печатает как `ошибка: ...`");
}
