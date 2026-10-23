use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio::time::sleep;

pub struct Job {
    pub id: u32,
    pub reply: oneshot::Sender<String>,
}

pub async fn serve(mut jobs: mpsc::Receiver<Job>, mut stop: watch::Receiver<bool>) {
    let mut in_flight = JoinSet::new();
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            Some(job) = jobs.recv() => {
                in_flight.spawn(async move {
                    sleep(Duration::from_millis(50)).await;
                    let _ = job.reply.send(format!("done {}", job.id));
                });
            }
        }
    }
    while in_flight.join_next().await.is_some() {}
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_requests_are_finished_before_exit() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap();
        let (jobs_tx, jobs_rx) = mpsc::channel(8);
        let (stop_tx, stop_rx) = watch::channel(false);
        let driver = async move {
            let mut replies = Vec::new();
            for id in 0..3 {
                let (reply, rx) = oneshot::channel();
                jobs_tx.send(Job { id, reply }).await.unwrap();
                replies.push(rx);
            }
            sleep(Duration::from_millis(10)).await;
            stop_tx.send(true).unwrap();
            replies
        };
        let (_, mut replies) = runtime.block_on(async { tokio::join!(serve(jobs_rx, stop_rx), driver) });
        drop(runtime);
        for (id, rx) in replies.iter_mut().enumerate() {
            assert_eq!(
                rx.try_recv().ok(),
                Some(format!("done {id}")),
                "запрос {id} был принят до остановки, и ответ на него должен дойти: serve обязан дождаться принятых"
            );
        }
    }

    #[test]
    fn serve_returns_when_idle() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap();
        let (_jobs_tx, jobs_rx) = mpsc::channel(1);
        let (stop_tx, stop_rx) = watch::channel(false);
        runtime.block_on(async {
            stop_tx.send(true).unwrap();
            tokio::time::timeout(Duration::from_secs(1), serve(jobs_rx, stop_rx))
                .await
                .expect("без работы serve должен выйти сразу после команды остановки");
        });
    }
}
