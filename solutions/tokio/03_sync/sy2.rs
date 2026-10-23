use std::sync::{Arc, Mutex};
use std::time::Duration;

async fn send(line: &str, sent: &Mutex<Vec<String>>) {
    tokio::time::sleep(Duration::from_millis(10)).await;
    sent.lock().unwrap().push(line.to_string());
}

async fn flush(pending: Arc<Mutex<Vec<String>>>, sent: Arc<Mutex<Vec<String>>>) -> usize {
    let lines = std::mem::take(&mut *pending.lock().unwrap());
    for line in &lines {
        send(line, &sent).await;
    }
    lines.len()
}

#[tokio::main]
async fn main() {
    let pending = Arc::new(Mutex::new(vec!["раз".to_string(), "два".to_string()]));
    let sent = Arc::new(Mutex::new(Vec::new()));
    let count = tokio::spawn(flush(pending, Arc::clone(&sent))).await.unwrap();
    println!("{count} {:?}", sent.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(items: &[&str]) -> Arc<Mutex<Vec<String>>> {
        Arc::new(Mutex::new(items.iter().map(|s| s.to_string()).collect()))
    }

    #[tokio::test(start_paused = true)]
    async fn sends_everything_in_order() {
        let pending = lines(&["a", "b", "c"]);
        let sent = lines(&[]);
        let count = tokio::spawn(flush(Arc::clone(&pending), Arc::clone(&sent))).await.unwrap();
        assert_eq!(count, 3, "flush возвращает число отправленных");
        assert_eq!(*sent.lock().unwrap(), ["a", "b", "c"], "порядок отправки сохраняется");
        assert!(pending.lock().unwrap().is_empty(), "очередь опустела");
    }

    #[tokio::test(start_paused = true)]
    async fn messages_added_during_the_flush_survive() {
        let pending = lines(&["a", "b", "c"]);
        let sent = lines(&[]);
        let job = tokio::spawn(flush(Arc::clone(&pending), Arc::clone(&sent)));
        tokio::time::sleep(Duration::from_millis(5)).await;
        pending.lock().unwrap().push("поздно1".to_string());
        pending.lock().unwrap().push("поздно2".to_string());
        assert_eq!(job.await.unwrap(), 3, "в этот flush попадает только то, что было на старте");
        assert_eq!(*sent.lock().unwrap(), ["a", "b", "c"], "новые сообщения не отправляются дважды и не теряются");
        assert_eq!(*pending.lock().unwrap(), ["поздно1", "поздно2"], "добавленное во время отправки ждёт следующего flush");
    }
}
