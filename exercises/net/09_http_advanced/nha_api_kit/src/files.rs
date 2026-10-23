use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{Method, Request, Response, Router, etag, handler, serve_bytes};

type Entry = (String, Vec<u8>);

#[derive(Clone, Default)]
pub struct FileStore {
    files: Arc<Mutex<HashMap<String, Entry>>>,
}

impl FileStore {
    pub fn new() -> FileStore {
        FileStore::default()
    }

    pub fn len(&self) -> usize {
        self.files.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub fn files_router(store: FileStore, max_upload: usize) -> Router {
    let _ = (store, max_upload);
    todo!()
}
