// Уведомления рассылаются в фоне: `spawn_send` запускает задачу, отдаёт `JoinHandle` и возвращается,
// а вызывающий код сразу идёт дальше (список пользователей и текст у него временные, а сам
// `Notifier` может вообще уничтожиться). Сейчас так не собирается: задача, судя по ошибке, держит
// чужие ссылки дольше, чем они живут.
// TODO: Почини `spawn_send`: задача должна быть самодостаточной.
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::sleep;

async fn deliver(log: &Mutex<Vec<String>>, delay_ms: u64, user: u32, text: &str) {
    sleep(Duration::from_millis(delay_ms)).await;
    log.lock().unwrap().push(format!("{user}:{text}"));
}

struct Notifier {
    log: Arc<Mutex<Vec<String>>>,
    delay_ms: u64,
}

impl Notifier {
    fn new(delay_ms: u64) -> Notifier {
        Notifier { log: Arc::new(Mutex::new(Vec::new())), delay_ms }
    }

    fn spawn_send(&self, users: &[u32], text: &str) -> JoinHandle<usize> {
        tokio::spawn(async move {
            for user in users {
                deliver(&self.log, self.delay_ms, *user, text).await;
            }
            users.len()
        })
    }
}

#[tokio::main]
async fn main() {
    let notifier = Notifier::new(5);
    let sent = notifier.spawn_send(&[1, 2], "привет").await.unwrap();
    println!("{sent} {:?}", notifier.log.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn task_owns_everything_it_needs() {
        let notifier = Notifier::new(10);
        let log = Arc::clone(&notifier.log);
        let handle = {
            let users = vec![1, 2, 3];
            let text = String::from("hi");
            notifier.spawn_send(&users, &text)
        };
        drop(notifier);
        assert_eq!(handle.await.unwrap(), 3, "задача должна вернуть число пользователей");
        assert_eq!(*log.lock().unwrap(), vec!["1:hi", "2:hi", "3:hi"], "рассылка идёт по порядку, даже когда вызвавший всё уничтожил");
    }

    #[tokio::test(start_paused = true)]
    async fn returns_at_once_and_delivers_in_the_background() {
        let notifier = Notifier::new(10);
        let handle = notifier.spawn_send(&[1, 2], "x");
        assert!(notifier.log.lock().unwrap().is_empty(), "spawn_send не должен ждать рассылку");
        handle.await.unwrap();
        assert_eq!(notifier.log.lock().unwrap().len(), 2, "после ожидания задачи оба сообщения доставлены");
    }

    #[tokio::test(start_paused = true)]
    async fn independent_sends_do_not_mix_their_inputs() {
        let notifier = Notifier::new(5);
        let first = notifier.spawn_send(&[1], "a");
        let second = notifier.spawn_send(&[2, 3], "b");
        assert_eq!((first.await.unwrap(), second.await.unwrap()), (1, 2), "каждая задача считает своих");
        let mut log = notifier.log.lock().unwrap().clone();
        log.sort();
        assert_eq!(log, vec!["1:a", "2:b", "3:b"], "у каждой задачи свой текст и свой список");
    }
}
