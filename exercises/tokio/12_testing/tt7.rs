// Сервер строчного протокола (PING, ADD a b, QUIT) проверяется через `tokio::io::duplex`: клиент и
// сервер в одной программе. Тест-сценарий отправляет все команды и сверяет ответы со стенограммой.
// На исправном сервере он почему-то не заканчивается вовремя, а на сломанных падает не с той
// ошибкой. Найди, почему обмен встаёт, и почини `exchange`. Проверки внизу (`mod tests`) не трогай.
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::time::timeout;

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    Good,
    NoNewline,
    WrongSum,
    KeepOpen,
}

trait Proto {
    const MODE: Mode;
}

struct Good;
struct NoNewline;
struct WrongSum;
struct KeepOpen;

impl Proto for Good {
    const MODE: Mode = Mode::Good;
}

impl Proto for NoNewline {
    const MODE: Mode = Mode::NoNewline;
}

impl Proto for WrongSum {
    const MODE: Mode = Mode::WrongSum;
}

impl Proto for KeepOpen {
    const MODE: Mode = Mode::KeepOpen;
}

async fn serve(stream: DuplexStream, mode: Mode) {
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut words = line.split_whitespace();
        let reply = match (words.next(), words.next(), words.next()) {
            (Some("PING"), _, _) => "PONG".to_string(),
            (Some("ADD"), Some(a), Some(b)) => match (a.parse::<i64>(), b.parse::<i64>()) {
                (Ok(a), Ok(b)) if mode == Mode::WrongSum => (a * b).to_string(),
                (Ok(a), Ok(b)) => (a + b).to_string(),
                _ => "ERR number".to_string(),
            },
            (Some("QUIT"), _, _) => {
                let _ = write.write_all(b"BYE\n").await;
                if mode != Mode::KeepOpen {
                    return;
                }
                continue;
            }
            _ => "ERR unknown".to_string(),
        };
        let tail = if mode == Mode::NoNewline { "" } else { "\n" };
        if write.write_all(format!("{reply}{tail}").as_bytes()).await.is_err() {
            return;
        }
    }
}


#[derive(Debug, PartialEq)]
enum Verdict {
    Passed,
    Failed,
    Hung,
}

fn verdict<F: FnOnce() + Send + 'static>(secs: u64, run: F) -> Verdict {
    let (done, finished) = std::sync::mpsc::channel();
    let worker = std::thread::Builder::new()
        .spawn(move || {
            run();
            let _ = done.send(());
        })
        .unwrap();
    match finished.recv_timeout(Duration::from_secs(secs)) {
        Ok(()) => {
            worker.join().unwrap();
            Verdict::Passed
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Verdict::Failed,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Verdict::Hung,
    }
}

async fn exchange<P: Proto>(commands: &[&str]) -> Vec<String> {
    let (mut client, server) = tokio::io::duplex(32);
    tokio::spawn(serve(server, P::MODE));
    for command in commands {
        client.write_all(format!("{command}\n").as_bytes()).await.unwrap();
    }
    let mut replies = Vec::new();
    let mut lines = BufReader::new(client).lines();
    while let Some(line) = lines.next_line().await.unwrap() {
        replies.push(line);
    }
    replies
}

fn suite<P: Proto>() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap();
    rt.block_on(async {
        let mut commands = vec!["PING", "ADD 2 3", "ADD 10 -4", "PING", "ADD 7 x", "HELLO", "ADD 100 200"];
        let mut expected = vec!["PONG", "5", "6", "PONG", "ERR number", "ERR unknown", "300"];
        commands.extend(std::iter::repeat_n("PING", 12));
        expected.extend(std::iter::repeat_n("PONG", 12));
        commands.push("QUIT");
        expected.push("BYE");
        let replies = timeout(Duration::from_secs(5), exchange::<P>(&commands)).await.expect("обмен завис: клиент и сервер ждут друг друга");
        assert_eq!(replies, expected, "стенограмма ответов сервера не совпала с ожидаемой");
    });
}

fn main() {
    suite::<Good>();
    println!("Good проходит");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suite_passes_on_the_correct_server() {
        assert_eq!(verdict(20, suite::<Good>), Verdict::Passed, "на исправном сервере сценарий обязан проходить");
    }

    #[test]
    fn the_suite_notices_a_reply_without_a_newline() {
        assert_eq!(verdict(20, suite::<NoNewline>), Verdict::Failed, "сервер без перевода строки в ответах должен быть пойман");
    }

    #[test]
    fn the_suite_notices_a_wrong_sum() {
        assert_eq!(verdict(20, suite::<WrongSum>), Verdict::Failed, "сервер, перемножающий вместо сложения, должен быть пойман");
    }

    #[test]
    fn the_suite_notices_a_connection_that_stays_open_after_quit() {
        assert_eq!(verdict(20, suite::<KeepOpen>), Verdict::Failed, "сервер, не закрывший соединение после QUIT, должен быть пойман");
    }
}
