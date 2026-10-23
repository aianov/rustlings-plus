use nudp_ping::{Report, ping};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

#[derive(Default, Clone)]
struct Chaos {
    stranger: HashSet<u32>,
    drop: HashSet<u32>,
    duplicate: HashSet<u32>,
    delay_ms: HashMap<u32, u64>,
    junk_before: HashSet<u32>,
}

async fn lossy_server(chaos: Chaos) -> (SocketAddr, JoinHandle<()>) {
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let addr = socket.local_addr().unwrap();
    let stranger = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let handle = tokio::spawn(async move {
        let mut buf = [0u8; 256];
        loop {
            let Ok((n, from)) = socket.recv_from(&mut buf).await else { return };
            if n < 4 {
                continue;
            }
            let packet = buf[..n].to_vec();
            let seq = u32::from_be_bytes([packet[0], packet[1], packet[2], packet[3]]);
            if chaos.drop.contains(&seq) {
                continue;
            }
            let socket = socket.clone();
            let stranger = stranger.clone();
            let chaos = chaos.clone();
            tokio::spawn(async move {
                if chaos.stranger.contains(&seq) {
                    let _ = stranger.send_to(&[0, 0, 0, 0, b'p'], from).await;
                }
                if chaos.junk_before.contains(&seq) {
                    let _ = socket.send_to(&[7, 7], from).await;
                    let _ = socket.send_to(&[0, 0, 1, 244, b'p'], from).await;
                }
                if let Some(ms) = chaos.delay_ms.get(&seq) {
                    sleep(Duration::from_millis(*ms)).await;
                }
                let _ = socket.send_to(&packet, from).await;
                if chaos.duplicate.contains(&seq) {
                    let _ = socket.send_to(&packet, from).await;
                }
            });
        }
    });
    (addr, handle)
}

async fn run(addr: SocketAddr, count: u32) -> (Report, Duration) {
    let start = Instant::now();
    let report = timeout(Duration::from_secs(5), ping(addr, count, Duration::from_millis(20), Duration::from_millis(400)))
        .await
        .expect("ping не вернул отчёт за 5 секунд: после последней отправки ждать надо не дольше wait")
        .expect("ping вернул ошибку");
    (report, start.elapsed())
}

#[tokio::test]
async fn clean_network_reports_zero_everything() {
    let (addr, srv) = lossy_server(Chaos::default()).await;
    let (r, _) = run(addr, 10).await;
    assert_eq!((r.sent, r.received, r.lost), (10, 10, 0), "на чистой сети должно быть 10 отправлено, 10 получено, 0 потеряно: {r:?}");
    assert_eq!((r.duplicates, r.reordered), (0, 0), "дублей и перестановок быть не должно: {r:?}");
    let (min, max, avg) = (r.rtt_min.expect("rtt_min"), r.rtt_max.expect("rtt_max"), r.rtt_avg.expect("rtt_avg"));
    assert!(min <= avg && avg <= max, "должно быть min <= avg <= max: {min:?} {avg:?} {max:?}");
    assert!(max < Duration::from_millis(200), "RTT на loopback не может быть {max:?}: считай время от отправки именно этого номера");
    assert_eq!(r.loss_percent(), 0.0);
    srv.abort();
}

#[tokio::test]
async fn loss_duplicates_and_reordering_are_counted() {
    let chaos = Chaos {
        drop: [2, 7].into(),
        duplicate: [5].into(),
        delay_ms: [(3, 70), (9, 250)].into(),
        junk_before: [1].into(),
        stranger: [4, 8].into(),
    };
    let (addr, srv) = lossy_server(chaos).await;
    let (r, elapsed) = run(addr, 10).await;
    assert_eq!(r.sent, 10, "отправлено должно быть 10: {r:?}");
    assert_eq!(r.received, 8, "получено должно быть 8 (номера 2 и 7 потеряны, мусор и дубль не в счёт): {r:?}");
    assert_eq!(r.lost, 2, "потеряно должно быть 2: {r:?}");
    assert_eq!(r.duplicates, 1, "дубль один (номер 5 пришёл дважды): {r:?}");
    assert_eq!(r.reordered, 1, "переставлен один ответ (номер 3 пришёл после 4, 5 и 6): {r:?}");
    let max = r.rtt_max.expect("rtt_max");
    assert!(max >= Duration::from_millis(240) && max < Duration::from_millis(450), "самый медленный ответ шёл 250 мс, а в отчёте {max:?}: опоздавший ответ тоже надо учесть, пока идёт wait");
    assert!(r.rtt_min.expect("rtt_min") < Duration::from_millis(60), "rtt_min слишком большой: {:?}", r.rtt_min);
    assert_eq!(r.loss_percent(), 20.0, "2 потери из 10 это 20 процентов");
    assert!(elapsed < Duration::from_millis(1000), "ping шёл {elapsed:?}: запросы уходят по таймеру, а не по одному с ожиданием ответа на каждый");
    srv.abort();
}

#[tokio::test]
async fn total_silence_is_total_loss() {
    let chaos = Chaos { drop: (0..5).collect(), ..Chaos::default() };
    let (addr, srv) = lossy_server(chaos).await;
    let start = Instant::now();
    let report = timeout(Duration::from_secs(5), ping(addr, 5, Duration::from_millis(20), Duration::from_millis(300)))
        .await
        .expect("ping не вернулся при полной тишине")
        .expect("ping вернул ошибку");
    assert_eq!((report.sent, report.received, report.lost), (5, 0, 5), "{report:?}");
    assert_eq!((report.rtt_min, report.rtt_max, report.rtt_avg), (None, None, None), "без ответов RTT нет");
    assert_eq!(report.loss_percent(), 100.0);
    assert!(start.elapsed() >= Duration::from_millis(280), "ping вернулся за {:?}: после последней отправки надо подождать wait, вдруг ответы просто опаздывают", start.elapsed());
    srv.abort();
}

#[tokio::test]
async fn zero_pings_is_an_empty_report() {
    let (addr, srv) = lossy_server(Chaos::default()).await;
    let report = timeout(Duration::from_secs(2), ping(addr, 0, Duration::from_millis(10), Duration::from_millis(50)))
        .await
        .expect("ping с count = 0 завис")
        .unwrap();
    assert_eq!(report, Report::default(), "ничего не отправляли, отчёт пустой");
    assert_eq!(report.loss_percent(), 0.0, "деление на ноль: при sent = 0 потерь нет");
    srv.abort();
}
