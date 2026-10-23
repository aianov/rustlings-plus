use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep_until};

pub trait Sink: Send + Sync + 'static {
    fn deliver(&self, user: u32, texts: Vec<String>);
}

#[derive(Default)]
pub struct Recorder {
    log: Mutex<Vec<(u32, Vec<String>)>>,
}

impl Recorder {
    pub fn new() -> Arc<Recorder> {
        Arc::new(Recorder::default())
    }

    pub fn deliveries(&self) -> Vec<(u32, Vec<String>)> {
        self.log.lock().unwrap().clone()
    }
}

impl Sink for Recorder {
    fn deliver(&self, user: u32, texts: Vec<String>) {
        self.log.lock().unwrap().push((user, texts));
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub window: Duration,
    pub max: usize,
}

#[derive(Debug, PartialEq)]
pub struct Closed;

enum Cmd {
    Notify(u32, String),
    Mute(u32),
    Unmute(u32),
    Tracked(oneshot::Sender<usize>),
    Finish,
}

#[derive(Clone)]
pub struct Notifier {
    tx: mpsc::Sender<Cmd>,
}

pub struct Runner {
    tx: mpsc::Sender<Cmd>,
    handle: JoinHandle<()>,
}

struct Digest {
    texts: Vec<String>,
    opened: Instant,
}

impl Notifier {
    pub fn start(sink: Arc<dyn Sink>, config: Config) -> (Notifier, Runner) {
        let (tx, rx) = mpsc::channel(64);
        let handle = tokio::spawn(run(rx, sink, config));
        (Notifier { tx: tx.clone() }, Runner { tx, handle })
    }

    pub async fn notify(&self, user: u32, text: &str) -> Result<(), Closed> {
        #[cfg(feature = "bug_closed_ok")]
        {
            let _ = self.tx.send(Cmd::Notify(user, text.to_string())).await;
            Ok(())
        }
        #[cfg(not(feature = "bug_closed_ok"))]
        {
            self.tx.send(Cmd::Notify(user, text.to_string())).await.map_err(|_| Closed)
        }
    }

    pub async fn mute(&self, user: u32) -> Result<(), Closed> {
        self.tx.send(Cmd::Mute(user)).await.map_err(|_| Closed)
    }

    pub async fn unmute(&self, user: u32) -> Result<(), Closed> {
        self.tx.send(Cmd::Unmute(user)).await.map_err(|_| Closed)
    }

    pub async fn tracked_users(&self) -> Result<usize, Closed> {
        let (answer, reply) = oneshot::channel();
        self.tx.send(Cmd::Tracked(answer)).await.map_err(|_| Closed)?;
        reply.await.map_err(|_| Closed)
    }
}

impl Runner {
    pub async fn finish(self) {
        let _ = self.tx.send(Cmd::Finish).await;
        let _ = self.handle.await;
    }
}

fn deliver(sink: &Arc<dyn Sink>, user: u32, mut texts: Vec<String>) {
    #[cfg(feature = "bug_order")]
    texts.reverse();
    #[cfg(not(feature = "bug_order"))]
    let _ = &mut texts;
    sink.deliver(user, texts);
}

fn flush_all(open: &mut HashMap<u32, Digest>, sink: &Arc<dyn Sink>) {
    #[cfg(feature = "bug_lost_tail")]
    {
        let _ = (open, sink);
    }
    #[cfg(not(feature = "bug_lost_tail"))]
    {
        let mut users: Vec<u32> = open.keys().copied().collect();
        users.sort_unstable();
        for user in users {
            if let Some(digest) = open.remove(&user) {
                if !digest.texts.is_empty() {
                    deliver(sink, user, digest.texts);
                }
            }
        }
    }
}

fn flush_due(open: &mut HashMap<u32, Digest>, sink: &Arc<dyn Sink>, window: Duration) {
    let now = Instant::now();
    let mut due: Vec<(Instant, u32)> =
        open.iter().filter(|(_, digest)| !digest.texts.is_empty() && digest.opened + window <= now).map(|(user, digest)| (digest.opened, *user)).collect();
    due.sort();
    for (_, user) in due {
        #[cfg(feature = "bug_leak")]
        {
            if let Some(digest) = open.get_mut(&user) {
                let texts = std::mem::take(&mut digest.texts);
                deliver(sink, user, texts);
            }
        }
        #[cfg(not(feature = "bug_leak"))]
        {
            if let Some(digest) = open.remove(&user) {
                deliver(sink, user, digest.texts);
            }
        }
    }
}

async fn run(mut rx: mpsc::Receiver<Cmd>, sink: Arc<dyn Sink>, config: Config) {
    let mut open: HashMap<u32, Digest> = HashMap::new();
    let mut muted: HashSet<u32> = HashSet::new();
    loop {
        let next_due = open.values().filter(|digest| !digest.texts.is_empty()).map(|digest| digest.opened + config.window).min();
        tokio::select! {
            cmd = rx.recv() => match cmd {
                None => {
                    flush_all(&mut open, &sink);
                    break;
                }
                Some(Cmd::Notify(user, text)) => {
                    if muted.contains(&user) {
                        continue;
                    }
                    let digest = open.entry(user).or_insert_with(|| Digest { texts: Vec::new(), opened: Instant::now() });
                    if digest.texts.is_empty() {
                        digest.opened = Instant::now();
                    }
                    #[cfg(feature = "bug_overwrite")]
                    digest.texts.clear();
                    #[cfg(feature = "bug_debounce")]
                    {
                        digest.opened = Instant::now();
                    }
                    digest.texts.push(text);
                    #[cfg(feature = "bug_off_by_one")]
                    let full = digest.texts.len() > config.max;
                    #[cfg(not(feature = "bug_off_by_one"))]
                    let full = digest.texts.len() >= config.max;
                    if full {
                        if let Some(digest) = open.remove(&user) {
                            deliver(&sink, user, digest.texts);
                        }
                    }
                }
                Some(Cmd::Mute(user)) => {
                    muted.insert(user);
                    #[cfg(not(feature = "bug_mute_pending"))]
                    open.remove(&user);
                }
                Some(Cmd::Unmute(user)) => {
                    muted.remove(&user);
                }
                Some(Cmd::Tracked(answer)) => {
                    let _ = answer.send(open.len());
                }
                Some(Cmd::Finish) => {
                    flush_all(&mut open, &sink);
                    break;
                }
            },
            _ = async { sleep_until(next_due.unwrap()).await }, if next_due.is_some() => {
                flush_due(&mut open, &sink, config.window);
            }
        }
    }
}
