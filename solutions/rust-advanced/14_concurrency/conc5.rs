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
    let (tx, rx) = mpsc::channel();
    for id in 0..producers {
        let tx = tx.clone();
        thread::spawn(move || {
            for k in 0..per_producer {
                tx.send(id * 1000 + k).expect("получатель жив");
            }
        });
    }
    drop(tx);
    let mut all: Vec<u32> = rx.iter().collect();
    all.sort_unstable();
    all
}

pub fn run_jobs(jobs: Vec<Job>) -> Vec<u32> {
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let (result_tx, result_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        for job in job_rx {
            result_tx.send(job()).expect("получатель жив");
        }
    });
    for job in jobs {
        job_tx.send(job).expect("рабочий жив");
    }
    drop(job_tx);
    let results = result_rx.iter().collect();
    worker.join().expect("рабочий упал");
    results
}

pub fn latest(rx: &Receiver<u32>) -> Option<u32> {
    rx.try_iter().last()
}

pub fn wait_for(rx: &Receiver<u32>, wait: Duration) -> Result<u32, Wait> {
    rx.recv_timeout(wait).map_err(|error| match error {
        RecvTimeoutError::Timeout => Wait::Timeout,
        RecvTimeoutError::Disconnected => Wait::Closed,
    })
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
