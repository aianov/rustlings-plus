use contracts::Entry;
use service_runtime::Stage;
use tokio::sync::{Mutex, MutexGuard, mpsc};

use super::{AppError, Op};
use crate::journal::{Journal, JournalWriter, Sink};

pub(super) struct Recorder {
    order: Mutex<()>,
    entries: Option<mpsc::Sender<Entry>>,
}

impl Recorder {
    pub(super) fn off() -> Recorder {
        Recorder {
            order: Mutex::new(()),
            entries: None,
        }
    }

    pub(super) fn start(
        stage: &Stage,
        journal: Journal,
        flush_every: usize,
        queue: usize,
    ) -> Recorder {
        let (entries, inbox) = mpsc::channel(queue.max(1));
        let writer = JournalWriter::new(journal.into_sink(), flush_every);
        stage.tasks.spawn(write(inbox, writer, stage.clone()));
        Recorder {
            order: Mutex::new(()),
            entries: Some(entries),
        }
    }

    pub(super) async fn order(&self) -> Option<MutexGuard<'_, ()>> {
        match self.entries {
            Some(..) => Some(self.order.lock().await),
            None => None,
        }
    }

    pub(super) async fn record(&self, entry: impl FnOnce() -> Entry) -> Result<(), AppError> {
        let Some(entries) = &self.entries else {
            return Ok(());
        };
        entries
            .send(entry())
            .await
            .map_err(|_| AppError::Stopped(Op::Journal))
    }
}

async fn write(mut inbox: mpsc::Receiver<Entry>, mut writer: JournalWriter<Sink>, stage: Stage) {
    loop {
        tokio::select! {
            biased;
            () = stage.token.cancelled() => break,
            next = inbox.recv() => match next {
                Some(entry) => {
                    if writer.append(&entry).await.is_err() {
                        return;
                    }
                }
                None => break,
            },
        }
    }
    inbox.close();
    while let Some(entry) = inbox.recv().await {
        if writer.append(&entry).await.is_err() {
            return;
        }
    }
    let _ = writer.close().await;
}
