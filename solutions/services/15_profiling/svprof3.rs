
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

fn main() {}

#[derive(Debug, PartialEq)]
pub struct Gauges {
    pub alive_tasks: usize,
    pub queue_depth: usize,
    pub workers: usize,
}

pub fn sample() -> Gauges {
    let metrics = tokio::runtime::Handle::current().metrics();
    Gauges { alive_tasks: metrics.num_alive_tasks(), queue_depth: metrics.global_queue_depth(), workers: metrics.num_workers() }
}

pub fn render(g: &Gauges) -> String {
    let mut out = String::new();
    for (name, help, value) in [
        ("tokio_alive_tasks", "Живые задачи рантайма", g.alive_tasks),
        ("tokio_global_queue_depth", "Задачи в общей очереди рантайма", g.queue_depth),
        ("tokio_workers", "Воркеры рантайма", g.workers),
    ] {
        out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}\n"));
    }
    out
}

#[derive(Default)]
pub struct Server {
    conns: Mutex<HashMap<u64, tokio::task::JoinHandle<()>>>,
    beats: Arc<AtomicU64>,
    next: AtomicU64,
}

impl Server {
    pub fn new() -> Server {
        Server::default()
    }

    pub fn connect(&self) -> u64 {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let beats = self.beats.clone();
        let ping = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(5)).await;
                beats.fetch_add(1, Ordering::SeqCst);
            }
        });
        self.conns.lock().expect("список не отравлен").insert(id, ping);
        id
    }

    pub async fn disconnect(&self, id: u64) {
        let ping = self.conns.lock().expect("список не отравлен").remove(&id);
        if let Some(ping) = ping {
            ping.abort();
            let _ = ping.await;
        }
    }

    pub fn connected(&self) -> usize {
        self.conns.lock().expect("список не отравлен").len()
    }

    pub fn beats(&self) -> u64 {
        self.beats.load(Ordering::SeqCst)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        for ping in self.conns.lock().expect("список не отравлен").values() {
            ping.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_gauges_see_tasks_come_and_go() {
        let base = sample();
        assert_eq!(base.workers, 1, "текущий рантайм однопоточный");
        let sleepers: Vec<_> = (0..5).map(|_| tokio::spawn(tokio::time::sleep(Duration::from_secs(30)))).collect();
        tokio::task::yield_now().await;
        assert_eq!(sample().alive_tasks, base.alive_tasks + 5, "пять спящих задач должны быть видны");
        for s in &sleepers {
            s.abort();
        }
        for s in sleepers {
            let _ = s.await;
        }
        assert_eq!(sample().alive_tasks, base.alive_tasks, "отменённые задачи исчезли из счёта");
    }

    #[test]
    fn the_text_format_has_help_type_and_value_for_each_gauge() {
        let text = render(&Gauges { alive_tasks: 42, queue_depth: 3, workers: 8 });
        assert!(text.contains("# HELP tokio_alive_tasks "), "{text}");
        assert!(text.contains("# TYPE tokio_alive_tasks gauge\ntokio_alive_tasks 42\n"), "{text}");
        assert!(text.contains("# TYPE tokio_global_queue_depth gauge\ntokio_global_queue_depth 3\n"), "{text}");
        assert!(text.contains("# TYPE tokio_workers gauge\ntokio_workers 8\n"), "{text}");
        assert!(text.ends_with('\n'));
    }

    #[tokio::test]
    async fn disconnecting_a_client_ends_its_background_task() {
        let server = Server::new();
        let base = sample().alive_tasks;
        let ids: Vec<u64> = (0..200).map(|_| server.connect()).collect();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(sample().alive_tasks, base + 200, "по задаче на подключение");
        for id in ids {
            server.disconnect(id).await;
        }
        assert_eq!(server.connected(), 0);
        assert_eq!(sample().alive_tasks, base, "после отключения всех клиентов живых задач столько же, сколько было: иначе это утечка, она растёт с каждым подключением");
    }

    #[tokio::test]
    async fn a_live_connection_keeps_pinging_and_a_closed_one_does_not() {
        let server = Server::new();
        let keep = server.connect();
        let gone = server.connect();
        tokio::time::sleep(Duration::from_millis(40)).await;
        server.disconnect(gone).await;
        let at_close = server.beats();
        assert!(at_close >= 4, "живые подключения пингуют: {at_close}");
        tokio::time::sleep(Duration::from_millis(40)).await;
        let later = server.beats();
        assert!(later > at_close, "оставшееся подключение продолжает работать");
        server.disconnect(keep).await;
        let stopped = server.beats();
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(server.beats(), stopped, "после отключения всех пинги прекращаются");
    }

    #[tokio::test]
    async fn dropping_the_server_leaves_nothing_behind() {
        let base = sample().alive_tasks;
        {
            let server = Server::new();
            for _ in 0..10 {
                server.connect();
            }
            tokio::task::yield_now().await;
            assert_eq!(sample().alive_tasks, base + 10);
        }
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
        assert_eq!(sample().alive_tasks, base, "остановка сервера должна останавливать и задачи подключений");
    }
}
