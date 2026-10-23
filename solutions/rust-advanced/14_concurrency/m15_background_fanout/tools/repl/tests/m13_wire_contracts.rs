use std::error::Error as _;
use std::io::{self, Read, Write};
use std::process::{Command as Process, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use contracts::{
    Changed, ChatHistory, ErrorBody, FeedPage, MessageView, Outcome, PostId, PostView, Push,
    Response, UserId, UserView, decode,
};
use repl::{Error, Mode, Terminal};
use social_core::{Notifiers, report};

const SCRIPT: &str = r#"{"id": 1, "service": "users", "method": "users.register", "data": {"username": "alice"}}
{"id": 2, "service": "users", "method": "users.register", "data": {"username": "bob"}}

{"id": 3, "service": "users", "method": "users.follow", "data": {"user": "u_2", "target": "u_1"}}
{"id": 4, "service": "posts", "method": "posts.create", "data": {"user": "u_1", "text": "Привет #rust"}}
{"id": 5, "service": "posts", "method": "posts.like", "data": {"user": "u_2", "post": "p_1"}}
{"id": 6, "service": "posts", "method": "posts.feed", "data": {"user": "u_2"}}
{"id": 7, "service": "chat", "method": "chat.send", "data": {"user": "u_2", "peer": "u_1", "text": "привет"}}
{"id": 8, "service": "chat", "method": "chat.history", "data": {"user": "u_1", "peer": "u_2", "limit": 5}}
{"id": 9, "service": "users", "method": "users.find", "data": {"username": "carol"}}
{"id": 10
"#;

const REGISTER: &str =
    r#"{"id": 1, "service": "users", "method": "users.register", "data": {"username": "alice"}}"#;

fn post_view(likes: usize) -> PostView {
    PostView {
        id: PostId(1),
        author: UserId(1),
        author_name: String::from("alice"),
        text: String::from("Привет #rust"),
        likes,
    }
}

fn message() -> MessageView {
    MessageView {
        from: UserId(2),
        to: UserId(1),
        text: String::from("привет"),
    }
}

fn user(id: u64, name: &str) -> UserView {
    UserView {
        id: UserId(id),
        username: String::from(name),
    }
}

fn ok<T>(id: u64, value: T) -> Response<T> {
    Response {
        id: Some(id),
        outcome: Outcome::Ok(value),
    }
}

fn failed(id: Option<u64>, code: &str, message: &str) -> Response<Changed> {
    Response {
        id,
        outcome: Outcome::Error(ErrorBody {
            code: String::from(code),
            message: String::from(message),
        }),
    }
}

fn check_script_output(out: &str) {
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.len(),
        14,
        "на 10 непустых строк приходит 10 ответов и 4 push-события, пустая строка пропускается, а пришло:\n{out}"
    );
    assert_eq!(
        decode::<Response<UserView>>(lines[0]),
        Ok(ok(1, user(1, "alice"))),
        "ответ на users.register, строка {:?}",
        lines[0]
    );
    assert_eq!(
        decode::<Response<UserView>>(lines[1]),
        Ok(ok(2, user(2, "bob"))),
        "второй ответ про bob, строка {:?}",
        lines[1]
    );
    assert_eq!(
        decode::<Response<Changed>>(lines[2]),
        Ok(ok(3, Changed { changed: true })),
        "ответ на users.follow, строка {:?}",
        lines[2]
    );
    assert_eq!(
        decode::<Push>(lines[3]),
        Ok(Push::Followed {
            to: UserId(1),
            by: UserId(2),
        }),
        "push о подписке идёт сразу после ответа на команду, которая его вызвала, строка {:?}",
        lines[3]
    );
    assert_eq!(
        decode::<Response<PostView>>(lines[4]),
        Ok(ok(4, post_view(0))),
        "ответ на posts.create, строка {:?}",
        lines[4]
    );
    assert_eq!(
        decode::<Push>(lines[5]),
        Ok(Push::NewPost {
            to: UserId(2),
            author: UserId(1),
            post: PostId(1),
        }),
        "подписчик bob узнаёт о новом посте alice push-событием сразу после ответа, строка {:?}",
        lines[5]
    );
    assert_eq!(
        decode::<Response<Changed>>(lines[6]),
        Ok(ok(5, Changed { changed: true })),
        "ответ на posts.like, строка {:?}",
        lines[6]
    );
    assert_eq!(
        decode::<Push>(lines[7]),
        Ok(Push::Liked {
            to: UserId(1),
            by: UserId(2),
            post: PostId(1),
        }),
        "push о лайке после ответа, строка {:?}",
        lines[7]
    );
    assert_eq!(
        decode::<Response<FeedPage>>(lines[8]),
        Ok(ok(
            6,
            FeedPage {
                posts: vec![post_view(1)],
            }
        )),
        "ответ на posts.feed, строка {:?}",
        lines[8]
    );
    assert_eq!(
        decode::<Response<MessageView>>(lines[9]),
        Ok(ok(7, message())),
        "ответ на chat.send, строка {:?}",
        lines[9]
    );
    assert_eq!(
        decode::<Push>(lines[10]),
        Ok(Push::Message {
            to: UserId(1),
            from: UserId(2),
        }),
        "push о личном сообщении после ответа, строка {:?}",
        lines[10]
    );
    assert_eq!(
        decode::<Response<ChatHistory>>(lines[11]),
        Ok(ok(
            8,
            ChatHistory {
                messages: vec![message()],
            }
        )),
        "ответ на chat.history, строка {:?}",
        lines[11]
    );
    assert_eq!(
        decode::<Response<Changed>>(lines[12]),
        Ok(failed(Some(9), "not_found", "нет пользователя @carol")),
        "незнакомый ник это not_found с id запроса, строка {:?}",
        lines[12]
    );
    assert_eq!(
        decode::<Response<Changed>>(lines[13]),
        Ok(failed(
            None,
            "truncated",
            "запрос не разобран: конверт оборван"
        )),
        "оборванный конверт получает ответ с id null, строка {:?}",
        lines[13]
    );
}

fn session(mode: Mode, input: &[u8]) -> (repl::Result<()>, String) {
    let mut out = Vec::new();
    let result = repl::run(mode, Notifiers::new(), input, &mut out);
    (
        result,
        String::from_utf8(out).expect("терминал пишет UTF-8"),
    )
}

#[test]
fn json_mode_speaks_envelopes() {
    let (result, out) = session(Mode::Json, SCRIPT.as_bytes());
    assert!(
        result.is_ok(),
        "сеанс без ошибок ввода и вывода завершается Ok"
    );
    check_script_output(&out);
}

#[test]
fn json_mode_goes_on_after_a_broken_line() {
    let input = format!("nope\n\n   \n{REGISTER}\n");
    let (result, out) = session(Mode::Json, input.as_bytes());
    assert!(result.is_ok(), "кривая строка не ломает сеанс");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "две непустые строки, два ответа:\n{out}");
    assert_eq!(
        decode::<Response<Changed>>(lines[0]),
        Ok(failed(
            None,
            "syntax",
            "запрос не разобран: не JSON (строка 1, столбец 2)"
        )),
        "не JSON это syntax с id null"
    );
    assert_eq!(
        decode::<Response<UserView>>(lines[1]),
        Ok(ok(1, user(1, "alice"))),
        "следующая строка исполняется как обычно"
    );
}

#[test]
fn text_mode_still_works() {
    let (result, out) = session(
        Mode::Text,
        "register alice\n\nfrobnicate\n/whoami\n".as_bytes(),
    );
    assert!(result.is_ok(), "текстовый сеанс завершается Ok");
    assert_eq!(
        out,
        "зарегистрирован @alice (id 1)\nошибка: неизвестная команда: frobnicate\nвход не выполнен\n",
        "текстовый режим отвечает как раньше: тело ответа или `ошибка: ...`"
    );
}

#[test]
fn read_error_names_the_line() {
    let mut input = Vec::from("register alice\n".as_bytes());
    input.extend_from_slice(b"\xff\xfe\n");
    input.extend_from_slice("register bob\n".as_bytes());
    let (result, out) = session(Mode::Text, &input);
    assert_eq!(
        out, "зарегистрирован @alice (id 1)\n",
        "до сломанной строки всё исполнено, после неё сеанс кончается"
    );
    let error = result.expect_err("байты не в UTF-8 это ошибка чтения");
    assert!(
        matches!(error, Error::Read { line: 2, .. }),
        "Error::Read знает номер строки (с единицы), а пришло {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "не удалось прочитать строку 2",
        "Display говорит только о своём уровне"
    );
    let cause = error
        .source()
        .and_then(|cause| cause.downcast_ref::<io::Error>())
        .expect("source у Read это io::Error");
    assert_eq!(
        cause.kind(),
        io::ErrorKind::InvalidData,
        "причина: не UTF-8"
    );
    assert_eq!(
        report(&error),
        format!("не удалось прочитать строку 2: {cause}"),
        "цепочка причин через report"
    );
}

struct Broken;

impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("канал закрыт"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn write_error_is_reported() {
    for mode in [Mode::Text, Mode::Json] {
        let input = if mode == Mode::Json {
            REGISTER
        } else {
            "register alice"
        };
        let error = repl::run(mode, Notifiers::new(), input.as_bytes(), Broken)
            .expect_err("вывод сломан, сеанс кончается ошибкой");
        assert!(
            matches!(error, Error::Write(..)),
            "{mode:?}: ошибка записи это Error::Write, а пришло {error:?}"
        );
        assert_eq!(
            report(&error),
            "не удалось записать ответ: канал закрыт",
            "{mode:?}: цепочка причин ошибки записи"
        );
    }
}

fn start_failed() -> repl::Result<()> {
    Err(social_core::Error::Start(io::Error::other("нет потоков")))?
}

fn repl_error<T: std::error::Error + Send + Sync + 'static>() {}

#[test]
fn core_errors_cross_the_border() {
    repl_error::<Error>();
    let error = start_failed().expect_err("ошибка ядра доезжает до терминала");
    assert!(
        matches!(error, Error::Core(social_core::Error::Start(..))),
        "From<social_core::Error> для repl::Error"
    );
    assert_eq!(
        report(&error),
        "ядро не запустилось: не запустился поток уведомлений: нет потоков",
        "каждый уровень называет только себя, причины идут через source"
    );
    let terminal: repl::Result<Terminal> = Terminal::new();
    assert!(terminal.is_ok(), "Terminal::new возвращает repl::Result");
}

fn binary(args: &[&str], input: &[u8]) -> (ExitStatus, String, String) {
    let mut child = Process::new(env!("CARGO_BIN_EXE_repl"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("бинарник repl (tools/repl) должен запускаться");
    let mut stdin = child.stdin.take().expect("у процесса есть stdin");
    let _ = stdin.write_all(input);
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait().expect("состояние процесса читается") {
            Some(status) => break status,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                panic!("терминал не завершился за 10 секунд после конца stdin");
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    let mut out = String::new();
    child
        .stdout
        .take()
        .expect("у процесса есть stdout")
        .read_to_string(&mut out)
        .expect("stdout читается");
    let mut err = String::new();
    child
        .stderr
        .take()
        .expect("у процесса есть stderr")
        .read_to_string(&mut err)
        .expect("stderr читается");
    (status, out, err)
}

#[test]
fn binary_has_a_json_flag() {
    let (status, out, err) = binary(&["--json"], SCRIPT.as_bytes());
    assert!(
        status.success(),
        "repl --json завершается успешно, когда stdin кончился; stderr:\n{err}"
    );
    check_script_output(&out);
}

#[test]
fn unknown_flag_is_an_error() {
    let (status, out, err) = binary(&["--wat"], b"register alice\n");
    assert!(!status.success(), "неизвестный флаг даёт код ошибки");
    assert_eq!(out, "", "с неизвестным флагом терминал ничего не исполняет");
    assert_eq!(
        err, "ошибка: неизвестный флаг --wat\n",
        "main печатает ошибку в stderr: `ошибка: <цепочка причин>`"
    );
}

#[test]
fn binary_prints_the_cause_chain() {
    let mut input = Vec::from(REGISTER.as_bytes());
    input.extend_from_slice(b"\n\xff\n");
    let (status, out, err) = binary(&["--json"], &input);
    assert!(
        !status.success(),
        "сломанный ввод завершает терминал с кодом ошибки"
    );
    assert_eq!(
        out.lines().count(),
        1,
        "первая строка исполнена до ошибки:\n{out}"
    );
    let prefix = "ошибка: не удалось прочитать строку 2: ";
    assert!(
        err.starts_with(prefix) && err.trim_end().len() > prefix.trim_end().len(),
        "main печатает всю цепочку причин из Box<dyn Error + Send + Sync>, а в stderr:\n{err}"
    );
}
