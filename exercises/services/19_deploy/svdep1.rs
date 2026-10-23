// Вся соцсеть это несколько процессов, и поднимать их десятью терминалами никто не хочет. Нужен свой лаунчер: он запускает процесс сервиса и
// собирает его вывод в одно место, помечая, чей он. Иначе в общем потоке не разобрать, какая строка от шлюза, а какая от NATS.
// Цель: `run_service` запускает программу, каждую строку её stdout пишет в приёмник как `[имя] строка`, а каждую строку stderr как `[имя!] строка`
// (без перевода строки на конце; последняя строка без перевода тоже попадает в приёмник), ждёт завершения процесса и возвращает его код выхода
// (`None`, если процесс убит сигналом). Много вывода не должно вешать процесс: канал между процессами небольшой.


use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

fn main() {}

pub type Sink = Arc<Mutex<Vec<String>>>;

async fn pump(reader: impl AsyncRead + Unpin, tag: String, sink: Sink) {
    let mut lines = BufReader::new(reader);
    let mut buf = String::new();
    loop {
        buf.clear();
        match lines.read_line(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(_) => sink.lock().unwrap().push(format!("{tag} {}", buf.trim_end_matches(['\n', '\r']))),
        }
    }
}

pub async fn run_service(name: &str, program: &str, args: &[&str], sink: Sink) -> std::io::Result<Option<i32>> {
    let _ = (name, &sink, Stdio::piped());
    let status = Command::new(program).args(args).status().await?;
    Ok(status.code())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    fn sink() -> Sink {
        Arc::new(Mutex::new(Vec::new()))
    }

    async fn sh(name: &str, script: &str, sink: &Sink) -> Option<i32> {
        timeout(Duration::from_secs(20), run_service(name, "sh", &["-c", script], sink.clone())).await.expect("лаунчер завис").unwrap()
    }

    #[tokio::test]
    async fn lines_are_prefixed_by_stream() {
        let s = sink();
        let code = sh("web", "echo one; echo two >&2; printf 'no newline'", &s).await;
        assert_eq!(code, Some(0));
        let lines = s.lock().unwrap().clone();
        assert!(lines.contains(&"[web] one".to_string()), "{lines:?}");
        assert!(lines.contains(&"[web!] two".to_string()), "{lines:?}");
        assert!(lines.contains(&"[web] no newline".to_string()), "последняя строка без перевода строки потерялась: {lines:?}");
        assert_eq!(lines.len(), 3);
    }

    #[tokio::test]
    async fn the_exit_code_is_reported() {
        let s = sink();
        assert_eq!(sh("a", "exit 3", &s).await, Some(3));
        assert_eq!(sh("a", "kill -9 $$", &s).await, None, "процесс убит сигналом: кода выхода нет");
    }

    #[tokio::test]
    async fn a_missing_program_is_an_error() {
        let r = run_service("x", "/no/such/program", &[], sink()).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn a_lot_of_output_does_not_hang_the_child() {
        let s = sink();
        let script = "i=0; while [ $i -lt 4000 ]; do echo \"stdout line number $i with some padding to make it longer\"; echo \"stderr line number $i with some padding to make it longer\" >&2; i=$((i+1)); done";
        let code = sh("loud", script, &s).await;
        assert_eq!(code, Some(0));
        let lines = s.lock().unwrap().clone();
        assert_eq!(lines.len(), 8000, "потеряны строки: {}", lines.len());
        assert_eq!(lines.iter().filter(|l| l.starts_with("[loud] ")).count(), 4000);
    }

    #[tokio::test]
    async fn two_services_do_not_mix_lines() {
        let s = sink();
        let script = |tag: &str| format!("i=0; while [ $i -lt 300 ]; do echo \"{tag} message $i\"; i=$((i+1)); done");
        let (a, b) = (script("alpha"), script("beta"));
        let (ra, rb) = tokio::join!(sh("one", &a, &s), sh("two", &b, &s));
        assert_eq!((ra, rb), (Some(0), Some(0)));
        let lines = s.lock().unwrap().clone();
        assert!(lines.iter().all(|l| l.starts_with("[one] alpha message ") || l.starts_with("[two] beta message ")), "строки перемешались посреди");
        assert_eq!(lines.len(), 600);
        let one: Vec<_> = lines.iter().filter(|l| l.starts_with("[one]")).collect();
        assert!(one.windows(2).all(|w| {
            let n = |s: &str| s.rsplit(' ').next().unwrap().parse::<u32>().unwrap();
            n(w[0]) < n(w[1])
        }), "порядок строк одного сервиса нарушен");
    }
}
