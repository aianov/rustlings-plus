use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use tokio::sync::broadcast;

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

#[derive(Clone)]
pub struct Store {}

pub struct Runner {}

impl Store {
    pub fn start(disk: Arc<dyn Disk>, config: Config) -> (Store, Runner) {
        todo!()
    }

    pub async fn publish(&self, author: u32, body: &str) -> Result<Post, Gone> {
        todo!()
    }

    pub async fn get(&self, id: u64) -> Result<Option<Post>, Gone> {
        todo!()
    }

    pub async fn by_author(&self, author: u32) -> Result<Vec<Post>, Gone> {
        todo!()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Post> {
        todo!()
    }
}

impl Runner {
    pub async fn finish(self) {
        todo!()
    }
}
