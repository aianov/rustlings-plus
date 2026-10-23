// Телефон вошёл в тоннель, и TCP-соединение «живо» только на бумаге: никто не прислал FIN, а писать в него бесполезно.
// Без проверки живости такие призраки висят в реестре часами. Сервер раз в interval шлёт строку `ping`, а клиент, пока жив,
// присылает хоть что-нибудь (`pong` или обычное сообщение). Нет вестей дольше idle_limit: соединение молчит, обработчик завершается.
// TODO: Допиши serve. Обычные строки клиента складывай в Report::lines (`pong` туда не попадает), и ни одна строка не должна теряться.

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::time::{Instant, interval_at, sleep_until};

#[derive(Clone, Copy)]
struct Heartbeat {
    interval: Duration,
    idle_limit: Duration,
}

#[derive(Debug, PartialEq)]
enum Exit {
    PeerClosed,
    Silent,
}

#[derive(Debug)]
struct Report {
    exit: Exit,
    lines: Vec<String>,
}

async fn serve<R, W>(reader: R, writer: W, config: Heartbeat) -> Report
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let _ = (reader, writer, config);
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{DuplexStream, duplex};
    use tokio::time::sleep;

    const CONFIG: Heartbeat = Heartbeat { interval: Duration::from_secs(30), idle_limit: Duration::from_secs(70) };

    fn start() -> (tokio::task::JoinHandle<Report>, DuplexStream) {
        let (server_side, client_side) = duplex(4096);
        let (r, w) = tokio::io::split(server_side);
        (tokio::spawn(serve(r, w, CONFIG)), client_side)
    }

    #[tokio::test(start_paused = true)]
    async fn silent_client_gets_two_pings_and_is_dropped_on_time() {
        let began = Instant::now();
        let (task, client) = start();
        let (cr, _cw) = tokio::io::split(client);
        let mut lines = BufReader::new(cr).lines();
        let mut ping_times = Vec::new();
        while ping_times.len() < 2 {
            let line = lines.next_line().await.unwrap().expect("сервер закрыл соединение до второго ping");
            assert_eq!(line, "ping");
            ping_times.push(began.elapsed());
        }
        assert!(ping_times[0] >= Duration::from_secs(30) && ping_times[0] < Duration::from_secs(31), "первый ping должен прийти через interval, а не сразу: {:?}", ping_times[0]);
        assert!(ping_times[1] >= Duration::from_secs(60) && ping_times[1] < Duration::from_secs(61), "второй ping через interval после первого: {:?}", ping_times[1]);
        let report = task.await.unwrap();
        let total = began.elapsed();
        assert_eq!(report.exit, Exit::Silent);
        assert!(total >= Duration::from_secs(70) && total < Duration::from_secs(75), "молчуна отключают через idle_limit от последней вести, а не раньше и не позже: {total:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn client_answering_pongs_lives_forever() {
        let began = Instant::now();
        let (task, client) = start();
        let (cr, mut cw) = tokio::io::split(client);
        let mut lines = BufReader::new(cr).lines();
        while began.elapsed() < Duration::from_secs(600) {
            let line = lines.next_line().await.unwrap().expect("живого клиента отключили");
            assert_eq!(line, "ping");
            cw.write_all(b"pong\n").await.unwrap();
        }
        cw.shutdown().await.unwrap();
        drop(lines);
        let report = task.await.unwrap();
        assert_eq!(report.exit, Exit::PeerClosed, "клиент отвечал на каждый ping, но сервер счёл его молчуном");
        assert!(report.lines.is_empty(), "pong это служебная строка, в сообщения она не попадает: {:?}", report.lines);
    }

    #[tokio::test(start_paused = true)]
    async fn ordinary_messages_count_as_life_signs() {
        let (task, client) = start();
        let (_cr, mut cw) = tokio::io::split(client);
        for n in 0..10 {
            sleep(Duration::from_secs(20)).await;
            cw.write_all(format!("привет {n}\n").as_bytes()).await.unwrap();
        }
        cw.shutdown().await.unwrap();
        let report = task.await.unwrap();
        assert_eq!(report.exit, Exit::PeerClosed, "клиент писал каждые 20 секунд, это живой клиент");
        assert_eq!(report.lines.len(), 10);
        assert_eq!(report.lines[9], "привет 9");
    }

    #[tokio::test(start_paused = true)]
    async fn half_a_line_survives_a_ping_tick() {
        let (task, client) = start();
        let (_cr, mut cw) = tokio::io::split(client);
        cw.write_all(b"hel").await.unwrap();
        sleep(Duration::from_secs(35)).await;
        cw.write_all(b"lo\n").await.unwrap();
        cw.shutdown().await.unwrap();
        let report = task.await.unwrap();
        assert_eq!(report.lines, vec!["hello".to_string()], "половина строки пропала, пока сработал таймер: чтение, прерванное таймером, теряет данные");
    }

    #[tokio::test(start_paused = true)]
    async fn half_a_character_survives_a_ping_tick() {
        let (task, client) = start();
        let (_cr, mut cw) = tokio::io::split(client);
        let bytes = "привет\n".as_bytes();
        cw.write_all(&bytes[..3]).await.unwrap();
        sleep(Duration::from_secs(35)).await;
        cw.write_all(&bytes[3..]).await.unwrap();
        cw.shutdown().await.unwrap();
        let report = task.await.unwrap();
        assert_eq!(report.lines, vec!["привет".to_string()], "символ разрезан между двумя порциями, а таймер сработал между ними");
    }
}
