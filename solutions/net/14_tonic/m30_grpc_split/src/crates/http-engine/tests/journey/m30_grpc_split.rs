use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use http_engine::{
    BoxFuture, Handler, Method, Request, Response, apply, bearer_auth_async, handler,
};

fn echo() -> Handler {
    handler(|request: Request| async move {
        let user = request
            .headers
            .get("x-user")
            .unwrap_or("аноним")
            .to_string();
        Response::text(200, &user)
    })
}

fn request(path: &str, token: Option<&str>) -> Request {
    let mut request = Request::new(Method::Get, path);
    if let Some(token) = token {
        request
            .headers
            .set("Authorization", &format!("Bearer {token}"));
    }
    request
}

fn text(response: &Response) -> String {
    String::from_utf8_lossy(&response.body).into_owned()
}

fn app(calls: Arc<AtomicUsize>) -> Handler {
    let verify = move |token: String| -> BoxFuture<Result<Option<String>, Response>> {
        let calls = Arc::clone(&calls);
        Box::pin(async move {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            match token.as_str() {
                "t-ada" => Ok(Some("ada".to_string())),
                "t-down" => Err(Response::error(503, "сервис входа недоступен")),
                _ => Ok(None),
            }
        })
    };
    let guard = |request: &Request| request.raw_path().starts_with("/me");
    apply(echo(), &[bearer_auth_async(verify, guard)])
}

#[tokio::test]
async fn async_bearer_checks_only_guarded_paths() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = app(Arc::clone(&calls));
    let open = app(request("/hello", Some("t-ada"))).await;
    assert_eq!(
        (
            open.status.code(),
            text(&open),
            calls.load(Ordering::SeqCst)
        ),
        (200, "аноним".to_string(), 0),
        "открытый путь не тратит сетевой вызов на проверку токена"
    );
    let mut forged = request("/hello", None);
    forged.headers.set("X-User", "admin");
    let forged = app(forged).await;
    assert_eq!(
        text(&forged),
        "аноним",
        "чужой X-User стирается и на открытом пути"
    );

    let missing = app(request("/me", None)).await;
    assert_eq!(missing.status.code(), 401);
    assert_eq!(missing.headers.get("www-authenticate"), Some("Bearer"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "без токена проверять нечего"
    );

    let wrong = app(request("/me", Some("t-nope"))).await;
    assert_eq!(wrong.status.code(), 401, "неизвестный токен");
    let down = app(request("/me", Some("t-down"))).await;
    assert_eq!(
        down.status.code(),
        503,
        "проверка сама решает, каким ответом закончить: сервис входа лёг, это не 401"
    );
    let ok = app(request("/me/avatar", Some("t-ada"))).await;
    assert_eq!(
        (ok.status.code(), text(&ok)),
        (200, "ada".to_string()),
        "проверенный пользователь виден обработчику в X-User"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "ровно по проверке на защищённый запрос с токеном"
    );
}
