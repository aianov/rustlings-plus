// Каналы `std::sync::mpsc` это очередь между потоками: много отправителей, один получатель. Канал закрывается,
// когда умирают все отправители, и именно тогда цикл по получателю заканчивается. Забудешь про одного
// отправителя, и программа зависнет навсегда. Задачи ниже про это и про очередь заданий.
// TODO: Допиши функции. Тесты ждут с таймаутом, поэтому зависание они покажут как ошибку.
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

pub type Job = Box<dyn FnOnce() -> u32 + Send>;

#[derive(Debug, PartialEq)]
pub enum Wait {
    Timeout,
    Closed,
}

pub fn collect_from_producers(producers: u32, per_producer: u32) -> Vec<u32> {
    todo!("producers потоков шлют id * 1000 + k для k от 0 до per_producer; вернуть всё полученное по возрастанию")
}

pub fn run_jobs(jobs: Vec<Job>) -> Vec<u32> {
    todo!("один рабочий поток получает задания по каналу и выполняет по порядку; вернуть результаты в том же порядке")
}

pub fn latest(rx: &Receiver<u32>) -> Option<u32> {
    todo!("забрать всё, что уже лежит в канале, не ожидая, и вернуть самое свежее")
}

pub fn wait_for(rx: &Receiver<u32>, wait: Duration) -> Result<u32, Wait> {
    todo!("значение, либо Timeout, если тихо, либо Closed, если отправителей не осталось")
}

fn main() {
    println!("{:?}", collect_from_producers(2, 3));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
            Ok(value) => value,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: не уложилось в {secs} с, похоже на зависание"),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: поток упал с паникой"),
        }
    }

    #[test]
    fn all_messages_from_all_producers_arrive() {
        let all = within(3, "сбор от производителей", || collect_from_producers(4, 5));
        assert_eq!(all.len(), 20, "должно прийти 4 * 5 сообщений");
        assert_eq!(all[0], 0);
        assert_eq!(all[19], 3004);
        assert!(all.windows(2).all(|pair| pair[0] < pair[1]), "результат по возрастанию, без повторов");
    }

    #[test]
    fn no_producers_means_an_instantly_closed_channel() {
        let all = within(3, "ноль производителей", || collect_from_producers(0, 5));
        assert!(all.is_empty(), "цикл по получателю должен кончиться, когда отправителей нет совсем");
    }

    #[test]
    fn jobs_run_in_order_on_one_worker() {
        let results = within(3, "очередь заданий", || {
            let jobs: Vec<Job> = vec![Box::new(|| 1), Box::new(|| 2 + 3), Box::new(|| 40 + 2)];
            run_jobs(jobs)
        });
        assert_eq!(results, vec![1, 5, 42]);
        assert!(within(3, "пустая очередь", || run_jobs(Vec::new())).is_empty());
    }

    #[test]
    fn jobs_run_on_another_thread() {
        let ids = within(3, "поток заданий", || {
            let here = thread::current().id();
            let job: Job = Box::new(move || u32::from(thread::current().id() == here));
            run_jobs(vec![job])
        });
        assert_eq!(ids, vec![0], "задание должно выполняться в рабочем потоке, а не в вызывающем");
    }

    #[test]
    fn latest_drains_whatever_is_pending() {
        let (tx, rx) = mpsc::channel();
        assert_eq!(latest(&rx), None);
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();
        assert_eq!(latest(&rx), Some(3));
        assert_eq!(latest(&rx), None, "канал опустошён");
    }

    #[test]
    fn waiting_tells_silence_from_closing() {
        let (tx, rx) = mpsc::channel();
        assert_eq!(wait_for(&rx, Duration::from_millis(20)), Err(Wait::Timeout), "отправитель жив, но молчит");
        tx.send(9).unwrap();
        assert_eq!(wait_for(&rx, Duration::from_millis(20)), Ok(9));
        drop(tx);
        assert_eq!(wait_for(&rx, Duration::from_millis(20)), Err(Wait::Closed), "все отправители ушли");
    }
}
