use crate::{Body, FileStore};
use std::io;

#[derive(Debug, PartialEq)]
pub struct Stored {
    pub key: String,
    pub size: u64,
    pub existed: bool,
}

pub async fn put_content<S: FileStore>(store: &S, body: Body, temp_key: &str) -> io::Result<Stored> {
    let _ = (store, body, temp_key);
    todo!()
}
