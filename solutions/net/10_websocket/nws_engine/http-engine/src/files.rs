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
    let reader = store.clone();
    let writer = store.clone();
    let remover = store;
    Router::new()
        .route(
            Method::Get,
            "/files/:name",
            handler(move |request: Request| {
                let store = reader.clone();
                async move {
                    let name = request.params.get("name").unwrap_or("").to_string();
                    let found = store.files.lock().unwrap().get(&name).cloned();
                    match found {
                        Some((content_type, data)) => serve_bytes(&request, &content_type, &data),
                        None => Response::error(404, "no such file"),
                    }
                }
            }),
        )
        .route(
            Method::Put,
            "/files/:name",
            handler(move |request: Request| {
                let store = writer.clone();
                async move {
                    if request.body.len() > max_upload {
                        return Response::error(413, "file too large");
                    }
                    let name = request.params.get("name").unwrap_or("").to_string();
                    let content_type = request.headers.get("content-type").unwrap_or("application/octet-stream").to_string();
                    let tag = etag(&request.body);
                    let created = store.files.lock().unwrap().insert(name, (content_type, request.body)).is_none();
                    let mut response = Response::new(if created { 201 } else { 204 });
                    response.headers.append("ETag", &tag);
                    response
                }
            }),
        )
        .route(
            Method::Delete,
            "/files/:name",
            handler(move |request: Request| {
                let store = remover.clone();
                async move {
                    let name = request.params.get("name").unwrap_or("").to_string();
                    match store.files.lock().unwrap().remove(&name) {
                        Some(_) => Response::new(204),
                        None => Response::error(404, "no such file"),
                    }
                }
            }),
        )
}
