use std::sync::{Arc, Mutex};

use http_engine::{Method, Request, Response, Router, Served, UNMATCHED, handler};

async fn ok(_: Request) -> Response {
    Response::text(200, "ok")
}

#[tokio::test]
async fn measured_router_reports_the_route_template() {
    let seen: Arc<Mutex<Vec<Served>>> = Arc::default();
    let sink = Arc::clone(&seen);
    let app = Router::new()
        .get("/users/:name", handler(ok))
        .get("/users/me", handler(ok))
        .post("/posts", handler(ok))
        .into_measured(Arc::new(move |served: &Served| {
            sink.lock().expect("замеры").push(served.clone());
        }));
    for (method, path) in [
        (Method::Get, "/users/alice"),
        (Method::Get, "/users/bob"),
        (Method::Get, "/users/me"),
        (Method::Get, "/posts"),
        (Method::Get, "/nowhere/42"),
        (Method::Head, "/users/carol"),
    ] {
        let _ = app(Request::new(method, path)).await;
    }
    let seen = seen.lock().expect("замеры").clone();
    let templates: Vec<(&str, u16)> = seen
        .iter()
        .map(|served| (served.template.as_str(), served.status))
        .collect();
    assert_eq!(
        templates,
        [
            ("/users/:name", 200),
            ("/users/:name", 200),
            ("/users/me", 200),
            (UNMATCHED, 405),
            (UNMATCHED, 404),
            ("/users/:name", 200),
        ],
        "в метке шаблон маршрута, а не путь: иначе каждый ник это новый временной ряд"
    );
    assert_eq!(seen[5].method, Method::Head);
}
