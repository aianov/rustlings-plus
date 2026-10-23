pub mod content;
pub mod image;
pub mod limit;
pub mod serve;
pub mod sign;
pub mod store;

use bytes::Bytes;
use futures::stream::BoxStream;
use std::future::Future;
use std::io;

pub const CHUNK: usize = 64 * 1024;

pub type Body = BoxStream<'static, io::Result<Bytes>>;

pub trait FileStore: Send + Sync {
    fn put(&self, key: &str, body: Body) -> impl Future<Output = io::Result<u64>> + Send;
    fn get(&self, key: &str) -> impl Future<Output = io::Result<Option<Body>>> + Send;
    fn exists(&self, key: &str) -> impl Future<Output = io::Result<bool>> + Send;
    fn rename(&self, from: &str, to: &str) -> impl Future<Output = io::Result<()>> + Send;
    fn delete(&self, key: &str) -> impl Future<Output = io::Result<bool>> + Send;
}
