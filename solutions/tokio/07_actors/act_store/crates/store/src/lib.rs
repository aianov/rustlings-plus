use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout_at};

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u64,
    pub author: u32,
    pub body: String,
}

pub trait Disk: Send + Sync + 'static {
    fn write(&self, batch: Vec<Post>) -> BoxFuture<'_, ()>;
}

#[derive(Debug, Clone)]
pub struct Config {
    pub batch_max: usize,
    pub batch_wait: Duration,
}

#[derive(Debug, PartialEq)]
pub struct Gone;

enum Cmd {
    Publish { author: u32, body: String, reply: oneshot::Sender<Post> },
    Get { id: u64, reply: oneshot::Sender<Option<Post>> },
    ByAuthor { author: u32, reply: oneshot::Sender<Vec<Post>> },
}

#[derive(Clone)]
pub struct Store {
    commands: mpsc::Sender<Cmd>,
    events: broadcast::Sender<Post>,
}

pub struct Runner {
    actor: JoinHandle<()>,
    writer: JoinHandle<()>,
}

impl Store {
    pub fn start(disk: Arc<dyn Disk>, config: Config) -> (Store, Runner) {
        let (commands, inbox) = mpsc::channel(64);
        let (to_disk, from_actor) = mpsc::channel(64);
        let (events, _) = broadcast::channel(64);
        let writer = tokio::spawn(write_loop(from_actor, disk, config));
        let actor = tokio::spawn(run_actor(inbox, to_disk, events.clone()));
        (Store { commands, events }, Runner { actor, writer })
    }

    pub async fn publish(&self, author: u32, body: &str) -> Result<Post, Gone> {
        let (reply, answer) = oneshot::channel();
        let cmd = Cmd::Publish { author, body: body.to_string(), reply };
        self.commands.send(cmd).await.map_err(|_| Gone)?;
        answer.await.map_err(|_| Gone)
    }

    pub async fn get(&self, id: u64) -> Result<Option<Post>, Gone> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Cmd::Get { id, reply }).await.map_err(|_| Gone)?;
        answer.await.map_err(|_| Gone)
    }

    pub async fn by_author(&self, author: u32) -> Result<Vec<Post>, Gone> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Cmd::ByAuthor { author, reply }).await.map_err(|_| Gone)?;
        answer.await.map_err(|_| Gone)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Post> {
        self.events.subscribe()
    }
}

impl Runner {
    pub async fn finish(self) {
        let _ = self.actor.await;
        let _ = self.writer.await;
    }
}

async fn run_actor(mut inbox: mpsc::Receiver<Cmd>, to_disk: mpsc::Sender<Post>, events: broadcast::Sender<Post>) {
    let mut posts: HashMap<u64, Post> = HashMap::new();
    let mut by_author: HashMap<u32, Vec<u64>> = HashMap::new();
    let mut next_id = 1;
    while let Some(cmd) = inbox.recv().await {
        match cmd {
            Cmd::Publish { author, body, reply } => {
                let post = Post { id: next_id, author, body };
                next_id += 1;
                posts.insert(post.id, post.clone());
                by_author.entry(author).or_default().push(post.id);
                let _ = events.send(post.clone());
                let _ = to_disk.send(post.clone()).await;
                let _ = reply.send(post);
            }
            Cmd::Get { id, reply } => {
                let _ = reply.send(posts.get(&id).cloned());
            }
            Cmd::ByAuthor { author, reply } => {
                let list = by_author
                    .get(&author)
                    .map(|ids| ids.iter().map(|id| posts[id].clone()).collect())
                    .unwrap_or_default();
                let _ = reply.send(list);
            }
        }
    }
}

async fn write_loop(mut incoming: mpsc::Receiver<Post>, disk: Arc<dyn Disk>, config: Config) {
    let mut batch: Vec<Post> = Vec::new();
    let mut deadline: Option<Instant> = None;
    loop {
        let next = match deadline {
            None => incoming.recv().await,
            Some(at) => match timeout_at(at, incoming.recv()).await {
                Ok(item) => item,
                Err(_) => {
                    disk.write(std::mem::take(&mut batch)).await;
                    deadline = None;
                    continue;
                }
            },
        };
        match next {
            Some(post) => {
                if batch.is_empty() {
                    deadline = Some(Instant::now() + config.batch_wait);
                }
                batch.push(post);
                if batch.len() >= config.batch_max {
                    disk.write(std::mem::take(&mut batch)).await;
                    deadline = None;
                }
            }
            None => break,
        }
    }
    if !batch.is_empty() {
        disk.write(batch).await;
    }
}
