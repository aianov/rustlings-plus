// Сердцебиение тикает раз в 5 мс, пока "база" отвечает на запрос. Всё идёт на одном
// потоке (так работает #[tokio::test] по умолчанию). Но поиск ждёт ответа базы через
// std::thread::sleep, и пока он спит, тикать некому: поток занят, соседние задачи стоят.
// TODO: Почини поиск, чтобы сердцебиение не замирало. Тест считает тики за время запроса.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

async fn heartbeat(beats: Arc<AtomicUsize>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        beats.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn lookup_with_heartbeat(id: u32) -> (String, usize) {
    let beats = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let pulse = tokio::spawn(heartbeat(Arc::clone(&beats), Arc::clone(&stop)));
    let name = tokio::spawn(slow_lookup(id)).await.unwrap();
    stop.store(true, Ordering::SeqCst);
    pulse.await.unwrap();
    (name, beats.load(Ordering::SeqCst))
}

async fn slow_lookup(id: u32) -> String {
    std::thread::sleep(Duration::from_millis(100));
    format!("пользователь {id}")
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (name, beats) = lookup_with_heartbeat(7).await;
    println!("{name}, тиков за время запроса: {beats}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lookup_still_returns_the_name() {
        let (name, _) = lookup_with_heartbeat(7).await;
        assert_eq!(name, "пользователь 7", "поиск обязан вернуть имя");
    }

    #[tokio::test]
    async fn heartbeat_keeps_ticking_during_lookup() {
        let (_, beats) = lookup_with_heartbeat(1).await;
        assert!(
            beats >= 10,
            "за 100 мс запроса сердце тикнуло только {beats} раз(а), а должно около 20: \
             поиск держит поток и соседние задачи не могут работать. Ждать надо через .await, а не усыплять поток"
        );
    }
}
