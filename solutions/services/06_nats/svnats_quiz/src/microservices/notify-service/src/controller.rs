use crate::common::{Config, Incoming, Stats};
use crate::dto::{Decoded, EventBody, decode};
use crate::service::{NotifyService, Recorded};
use nats_engine::{Bus, BusError, Pattern, Seg, Server};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

pub struct Controller {
    stop_intake: tokio::sync::oneshot::Sender<()>,
    intake: JoinHandle<()>,
    workers: Vec<JoinHandle<()>>,
    servers: Vec<Server>,
}

pub async fn start(bus: Arc<Bus>, config: &Config, service: Arc<NotifyService>, stats: Arc<Stats>) -> Result<Controller, BusError> {
    let root = config.root.as_str();
    let events = Pattern::build(&[Seg::Word(root), Seg::Any, Seg::Any, Seg::Any]).map_err(|e| BusError::Transport(e.to_string()))?;
    let mut subscription = bus.subscribe_group::<EventBody>(&events, &config.group).await?;

    let (queue_tx, queue_rx) = mpsc::channel::<Incoming>(config.queue);
    let queue_rx = Arc::new(Mutex::new(queue_rx));
    let mut workers = Vec::new();
    for _ in 0..config.workers {
        let (queue_rx, service, stats, deliver) = (queue_rx.clone(), service.clone(), stats.clone(), config.deliver.clone());
        workers.push(tokio::spawn(async move {
            loop {
                let next = queue_rx.lock().await.recv().await;
                let Some(incoming) = next else { return };
                match service.record(incoming) {
                    Recorded::Stored(notification) => {
                        Stats::bump(&stats.stored);
                        deliver(notification).await;
                    }
                    Recorded::Duplicate => Stats::bump(&stats.duplicates),
                }
            }
        }));
    }

    let (stop_intake, mut stopped) = tokio::sync::oneshot::channel();
    let intake = {
        let (root, stats) = (config.root.clone(), stats.clone());
        tokio::spawn(async move {
            let mut closing = false;
            loop {
                let item = if closing {
                    subscription.next().await
                } else {
                    tokio::select! {
                        item = subscription.next() => item,
                        _ = &mut stopped => {
                            subscription.stop().await;
                            closing = true;
                            continue;
                        }
                    }
                };
                let Some(item) = item else { break };
                Stats::bump(&stats.received);
                let envelope = match item {
                    Ok(envelope) => envelope,
                    Err(_) => {
                        Stats::bump(&stats.rejected);
                        continue;
                    }
                };
                match decode(&root, &envelope) {
                    Decoded::Ignore => Stats::bump(&stats.ignored),
                    Decoded::Reject(_) => Stats::bump(&stats.rejected),
                    Decoded::Notify(incoming) => match queue_tx.try_send(incoming) {
                        Ok(()) => Stats::bump(&stats.queued),
                        Err(_) => Stats::bump(&stats.dropped),
                    },
                }
            }
        })
    };

    let mut servers = Vec::new();
    let unread = Pattern::build(&[Seg::Word(root), Seg::Word("unread"), Seg::Any]).map_err(|e| BusError::Transport(e.to_string()))?;
    let reader = service.clone();
    servers.push(
        bus.serve::<serde_json::Value, u32, _>(&unread, Some(&config.group), move |request| Ok(reader.unread(request.subject.last_token()))).await?,
    );
    let read = Pattern::build(&[Seg::Word(root), Seg::Word("read"), Seg::Any]).map_err(|e| BusError::Transport(e.to_string()))?;
    let marker = service.clone();
    servers.push(
        bus.serve::<serde_json::Value, u32, _>(&read, Some(&config.group), move |request| Ok(marker.mark_read(request.subject.last_token()))).await?,
    );

    Ok(Controller { stop_intake, intake, workers, servers })
}

impl Controller {
    pub async fn stop(self) {
        self.stop_intake.send(()).ok();
        self.intake.await.ok();
        for server in self.servers {
            server.stop().await;
        }
        for worker in self.workers {
            worker.await.ok();
        }
    }
}
