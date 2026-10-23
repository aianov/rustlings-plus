use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Debug)]
enum Control {
    Pause,
    Resume,
}

async fn worker(mut jobs: mpsc::Receiver<u32>, mut control: mpsc::Receiver<Control>, done: Arc<Mutex<Vec<u32>>>) {
    let mut paused = false;
    loop {
        tokio::select! {
            Some(job) = jobs.recv(), if !paused => done.lock().unwrap().push(job),
            command = control.recv() => match command {
                Some(Control::Pause) => paused = true,
                Some(Control::Resume) => paused = false,
                None => break,
            },
        }
    }
}

#[tokio::main]
async fn main() {
    let (jobs_tx, jobs) = mpsc::channel(4);
    let (control_tx, control) = mpsc::channel(4);
    let done = Arc::new(Mutex::new(Vec::new()));
    let handle = tokio::spawn(worker(jobs, control, Arc::clone(&done)));
    jobs_tx.send(1).await.unwrap();
    drop(control_tx);
    handle.await.unwrap();
    println!("{:?}", done.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::mpsc::error::TrySendError;
    use tokio::time::sleep;

    async fn settle() {
        sleep(Duration::from_millis(10)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn works_without_pause() {
        let (jobs_tx, jobs) = mpsc::channel(4);
        let (control_tx, control) = mpsc::channel(4);
        let done = Arc::new(Mutex::new(Vec::new()));
        let handle = tokio::spawn(worker(jobs, control, Arc::clone(&done)));
        for job in [1, 2, 3] {
            jobs_tx.send(job).await.unwrap();
        }
        settle().await;
        assert_eq!(*done.lock().unwrap(), vec![1, 2, 3], "без паузы задания обрабатываются по порядку");
        drop(control_tx);
        handle.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn pause_keeps_jobs_in_the_queue() {
        let (jobs_tx, jobs) = mpsc::channel(2);
        let (control_tx, control) = mpsc::channel(4);
        let done = Arc::new(Mutex::new(Vec::new()));
        let handle = tokio::spawn(worker(jobs, control, Arc::clone(&done)));
        control_tx.send(Control::Pause).await.unwrap();
        settle().await;
        jobs_tx.try_send(1).unwrap();
        jobs_tx.try_send(2).unwrap();
        settle().await;
        assert!(done.lock().unwrap().is_empty(), "на паузе задания обрабатываться не должны");
        assert!(
            matches!(jobs_tx.try_send(3), Err(TrySendError::Full(3))),
            "очередь должна оставаться полной: на паузе воркер не должен забирать задания и копить их у себя"
        );
        control_tx.send(Control::Resume).await.unwrap();
        settle().await;
        assert_eq!(*done.lock().unwrap(), vec![1, 2], "после возобновления задания обрабатываются в прежнем порядке, ничего не потеряно");
        jobs_tx.try_send(3).unwrap();
        settle().await;
        assert_eq!(*done.lock().unwrap(), vec![1, 2, 3], "после паузы воркер работает как обычно");
        drop(control_tx);
        handle.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn control_is_heard_even_when_idle_or_closed_jobs() {
        let (jobs_tx, jobs) = mpsc::channel(2);
        let (control_tx, control) = mpsc::channel(4);
        let done = Arc::new(Mutex::new(Vec::new()));
        let handle = tokio::spawn(worker(jobs, control, Arc::clone(&done)));
        drop(jobs_tx);
        settle().await;
        control_tx.send(Control::Pause).await.unwrap();
        control_tx.send(Control::Resume).await.unwrap();
        drop(control_tx);
        let finished = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(finished.is_ok(), "канал команд закрыт: воркер обязан завершиться, даже если заданий уже нет");
    }
}
