use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::{AbortHandle, JoinHandle, JoinSet};
use tokio::time::{Instant, sleep_until};

pub type JobId = u64;
pub type JobFn = Box<dyn Fn() -> BoxFuture<'static, Result<String, String>> + Send + Sync>;

pub struct JobSpec {
    pub name: String,
    pub priority: u8,
    pub timeout: Duration,
    pub run: JobFn,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub workers: usize,
    pub retries: u32,
    pub backoff: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Queued,
    Running,
    Retrying(u32),
    Done(String),
    Failed(String),
    TimedOut,
    Cancelled,
}

impl Status {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Status::Done(_) | Status::Failed(_) | Status::TimedOut | Status::Cancelled)
    }
}

#[derive(Debug, PartialEq)]
pub struct Closed;

#[derive(Debug, PartialEq)]
pub struct Report {
    pub completed: usize,
    pub cancelled: usize,
}

enum Cmd {
    Submit(JobSpec, oneshot::Sender<JobId>),
    Status(JobId, oneshot::Sender<Option<Status>>),
    Cancel(JobId, oneshot::Sender<bool>),
    Wait(JobId, oneshot::Sender<Option<Status>>),
    Shutdown(Duration, oneshot::Sender<Report>),
}

enum Event {
    Progress(JobId, Status),
    Finished(JobId, Status),
}

#[derive(Clone)]
pub struct Scheduler {
    tx: mpsc::Sender<Cmd>,
    updates: broadcast::Sender<(JobId, Status)>,
}

pub struct Runner {
    tx: mpsc::Sender<Cmd>,
    handle: JoinHandle<()>,
}

impl Scheduler {
    pub fn start(config: Config) -> (Scheduler, Runner) {
        let (tx, rx) = mpsc::channel(64);
        let (updates, _) = broadcast::channel(4096);
        let handle = tokio::spawn(run(rx, config, updates.clone()));
        (Scheduler { tx: tx.clone(), updates }, Runner { tx, handle })
    }

    pub async fn submit(&self, spec: JobSpec) -> Result<JobId, Closed> {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Cmd::Submit(spec, reply)).await.map_err(|_| Closed)?;
        answer.await.map_err(|_| Closed)
    }

    pub async fn status(&self, id: JobId) -> Result<Option<Status>, Closed> {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Cmd::Status(id, reply)).await.map_err(|_| Closed)?;
        answer.await.map_err(|_| Closed)
    }

    pub async fn cancel(&self, id: JobId) -> Result<bool, Closed> {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Cmd::Cancel(id, reply)).await.map_err(|_| Closed)?;
        answer.await.map_err(|_| Closed)
    }

    pub async fn wait(&self, id: JobId) -> Result<Option<Status>, Closed> {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Cmd::Wait(id, reply)).await.map_err(|_| Closed)?;
        answer.await.map_err(|_| Closed)
    }

    pub fn updates(&self) -> BoxStream<'static, (JobId, Status)> {
        let receiver = self.updates.subscribe();
        futures::stream::unfold(receiver, |mut receiver| async move {
            loop {
                match receiver.recv().await {
                    Ok(update) => return Some((update, receiver)),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        })
        .boxed()
    }
}

impl Runner {
    pub async fn shutdown(self, grace: Duration) -> Report {
        let (reply, answer) = oneshot::channel();
        let _ = self.tx.send(Cmd::Shutdown(grace, reply)).await;
        let report = answer.await.unwrap_or(Report { completed: 0, cancelled: 0 });
        let _ = self.handle.await;
        report
    }
}

struct Entry {
    priority: u8,
    seq: u64,
    status: Status,
    timeout: Duration,
    run: Option<Arc<JobFn>>,
    abort: Option<AbortHandle>,
}

struct Actor {
    config: Config,
    jobs: HashMap<JobId, Entry>,
    queue: Vec<JobId>,
    running: usize,
    next_id: JobId,
    waiters: HashMap<JobId, Vec<oneshot::Sender<Option<Status>>>>,
    set: JoinSet<()>,
    events: mpsc::UnboundedSender<Event>,
    updates: broadcast::Sender<(JobId, Status)>,
}

impl Actor {
    fn set_status(&mut self, id: JobId, status: Status) {
        if let Some(entry) = self.jobs.get_mut(&id) {
            entry.status = status.clone();
        }
        let _ = self.updates.send((id, status.clone()));
        if status.is_terminal() {
            for waiter in self.waiters.remove(&id).unwrap_or_default() {
                let _ = waiter.send(Some(status.clone()));
            }
        }
    }

    fn pump(&mut self) {
        while self.running < self.config.workers {
            let best = self.queue.iter().copied().max_by_key(|id| {
                let entry = &self.jobs[id];
                (entry.priority, std::cmp::Reverse(entry.seq))
            });
            let Some(id) = best else { break };
            self.queue.retain(|queued| *queued != id);
            let entry = self.jobs.get_mut(&id).expect("задание есть");
            let run = entry.run.take().expect("задание ещё не запускали");
            let timeout = entry.timeout;
            let abort = self.set.spawn(attempts(id, run, timeout, self.config.clone(), self.events.clone()));
            self.jobs.get_mut(&id).expect("задание есть").abort = Some(abort);
            self.running += 1;
            self.set_status(id, Status::Running);
        }
    }

    fn finish_running(&mut self, id: JobId) {
        if let Some(entry) = self.jobs.get_mut(&id) {
            entry.abort = None;
        }
        self.running -= 1;
    }

    fn report(&self) -> Report {
        let mut report = Report { completed: 0, cancelled: 0 };
        for entry in self.jobs.values() {
            match entry.status {
                Status::Done(_) | Status::Failed(_) | Status::TimedOut => report.completed += 1,
                Status::Cancelled => report.cancelled += 1,
                _ => {}
            }
        }
        report
    }
}

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<&'static str>() {
        Ok(text) => text.to_string(),
        Err(other) => match other.downcast::<String>() {
            Ok(text) => *text,
            Err(_) => "неизвестная паника".to_string(),
        },
    }
}

async fn attempts(id: JobId, run: Arc<JobFn>, timeout: Duration, config: Config, events: mpsc::UnboundedSender<Event>) {
    let mut retries_done = 0u32;
    loop {
        let make: &JobFn = &run;
        let attempt = AssertUnwindSafe(tokio::time::timeout(timeout, make())).catch_unwind().await;
        let verdict = match attempt {
            Err(payload) => Status::Failed(format!("panic: {}", panic_text(payload))),
            Ok(Err(_)) => Status::TimedOut,
            Ok(Ok(Ok(text))) => Status::Done(text),
            Ok(Ok(Err(error))) => {
                if retries_done < config.retries {
                    retries_done += 1;
                    let _ = events.send(Event::Progress(id, Status::Retrying(retries_done)));
                    tokio::time::sleep(config.backoff * 2u32.pow(retries_done - 1)).await;
                    let _ = events.send(Event::Progress(id, Status::Running));
                    continue;
                }
                Status::Failed(error)
            }
        };
        let _ = events.send(Event::Finished(id, verdict));
        return;
    }
}

async fn run(mut commands: mpsc::Receiver<Cmd>, config: Config, updates: broadcast::Sender<(JobId, Status)>) {
    let (events_tx, mut events) = mpsc::unbounded_channel();
    let mut actor = Actor {
        config,
        jobs: HashMap::new(),
        queue: Vec::new(),
        running: 0,
        next_id: 1,
        waiters: HashMap::new(),
        set: JoinSet::new(),
        events: events_tx,
        updates,
    };
    let mut shutdown: Option<(Instant, oneshot::Sender<Report>)> = None;
    loop {
        if shutdown.is_some() && actor.running == 0 {
            if let Some((_, reply)) = shutdown.take() {
                let _ = reply.send(actor.report());
            }
            break;
        }
        let deadline = shutdown.as_ref().map(|(deadline, _)| *deadline);
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    Cmd::Submit(spec, reply) => {
                        if shutdown.is_some() {
                            continue;
                        }
                        let id = actor.next_id;
                        actor.next_id += 1;
                        actor.jobs.insert(
                            id,
                            Entry { priority: spec.priority, seq: id, status: Status::Queued, timeout: spec.timeout, run: Some(Arc::new(spec.run)), abort: None },
                        );
                        actor.queue.push(id);
                        actor.set_status(id, Status::Queued);
                        let _ = reply.send(id);
                        actor.pump();
                    }
                    Cmd::Status(id, reply) => {
                        let _ = reply.send(actor.jobs.get(&id).map(|entry| entry.status.clone()));
                    }
                    Cmd::Cancel(id, reply) => {
                        let status = actor.jobs.get(&id).map(|entry| entry.status.clone());
                        let cancelled = match status {
                            Some(Status::Queued) => {
                                actor.queue.retain(|queued| *queued != id);
                                actor.set_status(id, Status::Cancelled);
                                true
                            }
                            Some(Status::Running | Status::Retrying(_)) => {
                                if let Some(abort) = actor.jobs.get(&id).and_then(|entry| entry.abort.clone()) {
                                    abort.abort();
                                }
                                actor.finish_running(id);
                                actor.set_status(id, Status::Cancelled);
                                actor.pump();
                                true
                            }
                            _ => false,
                        };
                        let _ = reply.send(cancelled);
                    }
                    Cmd::Wait(id, reply) => match actor.jobs.get(&id).map(|entry| entry.status.clone()) {
                        None => {
                            let _ = reply.send(None);
                        }
                        Some(status) if status.is_terminal() => {
                            let _ = reply.send(Some(status));
                        }
                        Some(_) => actor.waiters.entry(id).or_default().push(reply),
                    },
                    Cmd::Shutdown(grace, reply) => {
                        if shutdown.is_some() {
                            continue;
                        }
                        let mut queued = std::mem::take(&mut actor.queue);
                        queued.sort_by_key(|id| actor.jobs[id].seq);
                        for id in queued {
                            actor.set_status(id, Status::Cancelled);
                        }
                        shutdown = Some((Instant::now() + grace, reply));
                    }
                }
            }
            event = events.recv() => {
                match event {
                    Some(Event::Progress(id, status)) => {
                        if actor.jobs.get(&id).is_some_and(|entry| !entry.status.is_terminal()) {
                            actor.set_status(id, status);
                        }
                    }
                    Some(Event::Finished(id, status)) => {
                        if actor.jobs.get(&id).is_some_and(|entry| !entry.status.is_terminal()) {
                            actor.finish_running(id);
                            actor.set_status(id, status);
                            actor.pump();
                        }
                    }
                    None => break,
                }
            }
            reaped = actor.set.join_next(), if !actor.set.is_empty() => {
                let _ = reaped;
            }
            _ = async { sleep_until(deadline.unwrap()).await }, if deadline.is_some() => {
                let running: Vec<JobId> = actor.jobs.iter().filter(|(_, entry)| matches!(entry.status, Status::Running | Status::Retrying(_))).map(|(id, _)| *id).collect();
                let mut running = running;
                running.sort_unstable();
                for id in running {
                    if let Some(abort) = actor.jobs.get(&id).and_then(|entry| entry.abort.clone()) {
                        abort.abort();
                    }
                    actor.finish_running(id);
                    actor.set_status(id, Status::Cancelled);
                }
                if let Some((_, reply)) = shutdown.take() {
                    let _ = reply.send(actor.report());
                }
                break;
            }
        }
    }
}
