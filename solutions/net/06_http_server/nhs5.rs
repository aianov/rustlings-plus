use std::sync::{Arc, Mutex};

use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Method {
    Get,
    Post,
    Put,
    Delete,
    Head,
}

#[allow(dead_code)]
impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Request {
    method: Method,
    path: String,
    body: String,
}

impl Request {
    fn new(method: Method, path: &str) -> Request {
        Request { method, path: path.to_string(), body: String::new() }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

#[allow(dead_code)]
impl Response {
    fn text(status: u16, body: &str) -> Response {
        Response { status, headers: Vec::new(), body: body.to_string() }
    }

    fn header(mut self, name: &str, value: &str) -> Response {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn get_header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type Handler = Box<dyn Fn(Request) -> BoxFuture<Response> + Send + Sync>;

fn handler<F, Fut>(f: F) -> Handler
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    Box::new(move |request| Box::pin(f(request)))
}

struct Note {
    id: u32,
    text: String,
}

struct AppState {
    notes: Mutex<Vec<Note>>,
    audit_log: tokio::sync::Mutex<Vec<String>>,
}

impl AppState {
    fn new() -> AppState {
        AppState { notes: Mutex::new(Vec::new()), audit_log: tokio::sync::Mutex::new(Vec::new()) }
    }

    async fn audit(&self, line: String) {
        tokio::task::yield_now().await;
        self.audit_log.lock().await.push(line);
    }
}

fn add_note(state: Arc<AppState>) -> Handler {
    handler(move |req| {
        let state = state.clone();
        async move {
            let text = req.body.trim().to_string();
            if text.is_empty() {
                return Response::text(400, "empty note");
            }
            let id = {
                let mut notes = state.notes.lock().unwrap();
                let id = notes.len() as u32 + 1;
                notes.push(Note { id, text });
                id
            };
            state.audit(format!("added {id}")).await;
            Response::text(201, &id.to_string())
        }
    })
}

fn list_notes(state: Arc<AppState>) -> Handler {
    handler(move |_req| {
        let state = state.clone();
        async move {
            let notes = state.notes.lock().unwrap();
            let body: String = notes.iter().map(|n| format!("{}: {}\n", n.id, n.text)).collect();
            Response::text(200, &body)
        }
    })
}

#[tokio::main]
async fn main() {
    let state = Arc::new(AppState::new());
    let add = add_note(state.clone());
    let mut request = Request::new(Method::Post, "/notes");
    request.body = "купить хлеб".to_string();
    println!("{}", add(request).await.body);
    println!("{}", list_notes(state)(Request::new(Method::Get, "/notes")).await.body);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(body: &str) -> Request {
        let mut request = Request::new(Method::Post, "/notes");
        request.body = body.to_string();
        request
    }

    #[tokio::test]
    async fn add_and_list() {
        let state = Arc::new(AppState::new());
        let (add, list) = (add_note(state.clone()), list_notes(state.clone()));
        assert_eq!(list(Request::new(Method::Get, "/notes")).await.body, "", "пустой список это пустое тело");
        let first = add(post("  молоко \n")).await;
        assert_eq!((first.status, first.body.as_str()), (201, "1"));
        assert_eq!(add(post("хлеб")).await.body, "2");
        assert_eq!(list(Request::new(Method::Get, "/notes")).await.body, "1: молоко\n2: хлеб\n", "текст хранится без пробелов по краям");
        for blank in ["", "   ", "\n\t"] {
            assert_eq!(add(post(blank)).await.status, 400, "{blank:?}: пустая заметка");
        }
        assert_eq!(state.notes.lock().unwrap().len(), 2, "отвергнутая заметка не должна сохраняться");
        assert_eq!(state.audit_log.lock().await.len(), 2, "в журнал пишутся только успешные добавления");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_adds_get_unique_ordered_ids() {
        let state = Arc::new(AppState::new());
        let add = Arc::new(add_note(state.clone()));
        let mut tasks = Vec::new();
        for i in 0..200 {
            let add = add.clone();
            tasks.push(tokio::spawn(async move { add(post(&format!("note {i}"))).await.body }));
        }
        let mut ids = Vec::new();
        for task in tasks {
            ids.push(task.await.unwrap().parse::<u32>().unwrap());
        }
        ids.sort_unstable();
        assert_eq!(ids, (1..=200).collect::<Vec<u32>>(), "номера должны быть уникальными и без дыр");
        let stored: Vec<u32> = state.notes.lock().unwrap().iter().map(|n| n.id).collect();
        assert_eq!(stored, (1..=200).collect::<Vec<u32>>(), "заметки должны лежать в порядке номеров: номер выдают под той же блокировкой, где кладут");
        assert_eq!(state.audit_log.lock().await.len(), 200);
    }
}
