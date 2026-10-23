use api_gateway::{Config, Ctx, Routes};
use contracts::{Envelope, ErrorBody, Outcome, PostView, Reply, UserView, decode};
use social_cli::{Reply as WebReply, run_with};

use crate::support::{Gateway, ok, within};

fn error(reply: &WebReply) -> ErrorBody {
    match decode::<Outcome<()>>(&reply.body) {
        Ok(Outcome::Error(body)) => body,
        other => panic!("ждали конверт ошибки, а пришло {other:?}: {reply:?}"),
    }
}

fn stamped(reply: &WebReply) -> ErrorBody {
    let body = error(reply);
    let rid = body
        .request_id
        .clone()
        .expect("в теле ошибки есть request_id");
    assert_eq!(
        reply.header("x-request-id"),
        Some(rid.as_str()),
        "номер в теле и в заголовке один: {reply:?}"
    );
    body
}

#[tokio::test]
async fn profile_patch_over_http_has_three_states() {
    let stack = Gateway::start(Config::default()).await;
    stack.register("alice").await;
    let token = stack.login("alice").await.token;
    let patch = |body: &'static str| send(&stack, &token, body);
    let set = patch(r#"{"display_name": "  Алиса  ", "bio": "пишу на Rust"}"#).await;
    assert_eq!(set.status, 200, "{set:?}");
    let user: UserView = decode(&set.body).expect("UserView");
    assert_eq!(
        (user.display_name.as_deref(), user.bio.as_deref()),
        (Some("Алиса"), Some("пишу на Rust"))
    );
    let cleared = patch(r#"{"bio": null}"#).await;
    let user: UserView = decode(&cleared.body).expect("UserView");
    assert_eq!(
        (user.display_name.as_deref(), user.bio.as_deref()),
        (Some("Алиса"), None),
        "null очищает поле, а неприсланное не трогается"
    );
    let untouched = patch("{}").await;
    assert_eq!(decode::<UserView>(&untouched.body).expect("UserView"), user);
    let blank = patch(r#"{"display_name": "", "bio": "ок"}"#).await;
    assert_eq!(blank.status, 422);
    assert_eq!(stamped(&blank).fields(), [("display_name", "blank")]);
    let me = within("GET /users/me", web_me(&stack, &token)).await;
    let user: UserView = decode(&me.body).expect("UserView");
    assert_eq!(
        (user.display_name.as_deref(), user.bio),
        (Some("Алиса"), None),
        "PATCH с ошибкой не применился даже частично"
    );
    let extra = patch(r#"{"username": "root"}"#).await;
    assert_eq!(
        (extra.status, stamped(&extra).fields()),
        (422, vec![("username", "unknown_field")]),
        "ник через PATCH не меняется: такого поля в DTO нет"
    );
    let broken = patch("{oops").await;
    assert_eq!(
        (broken.status, stamped(&broken).code.as_str()),
        (400, "syntax")
    );
    stack.stop().await;
}

async fn send(stack: &Gateway, token: &str, body: &str) -> WebReply {
    within(
        "PATCH /users/me",
        stack
            .web()
            .request("PATCH", "/users/me", Some(body), Some(token)),
    )
    .await
    .expect("шлюз отвечает")
}

async fn web_me(stack: &Gateway, token: &str) -> WebReply {
    stack
        .web()
        .request("GET", "/users/me", None, Some(token))
        .await
        .expect("шлюз отвечает")
}

#[tokio::test]
async fn every_http_error_wears_the_same_envelope() {
    let stack = Gateway::start(Config::default()).await;
    let mut web = stack.web();
    let anonymous = web
        .request("PATCH", "/users/me", Some("{}"), None)
        .await
        .expect("ответ");
    assert_eq!(anonymous.status, 401);
    assert_eq!(stamped(&anonymous).code, "unauthorized");
    let nowhere = web
        .request("GET", "/nowhere", None, None)
        .await
        .expect("ответ");
    assert_eq!(nowhere.status, 404);
    assert_eq!(
        stamped(&nowhere).code,
        "not_found",
        "даже 404 роутера в конверте"
    );
    let bad_id = web
        .request("GET", "/posts/zzz", None, None)
        .await
        .expect("ответ");
    assert_eq!(
        (bad_id.status, stamped(&bad_id).code.as_str()),
        (400, "bad_data")
    );
    let fine = web
        .request("GET", "/health", None, None)
        .await
        .expect("ответ");
    assert!(
        fine.header("x-request-id")
            .is_some_and(|rid| rid.len() == 16),
        "номер запроса есть и у успешного ответа: {fine:?}"
    );
    stack.stop().await;
}

#[tokio::test]
async fn posts_carry_tags_and_links_and_report_fields() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut peer) = stack.member("alice").await;
    let post = ok!(
        peer,
        "posts.create",
        r##"{"text": "читаю про tokio", "tags": ["#Rust", "async"], "links": [{"url": "https://tokio.rs", "title": "Сайт"}]}"##
            => PostView
    );
    assert_eq!(post.author, alice.id);
    assert_eq!(post.tags, ["rust", "async"]);
    assert_eq!(post.links[0].url, "https://tokio.rs");
    assert_eq!(post.links[0].title.as_deref(), Some("Сайт"));
    let refused = peer
        .fails(
            "posts.create",
            r#"{"text": "  ", "tags": ["c++"], "links": [{"url": "javascript:alert(1)"}]}"#,
        )
        .await;
    assert_eq!(refused.code, "validation");
    assert_eq!(
        refused.fields(),
        [
            ("text", "blank"),
            ("tags[0]", "invalid_format"),
            ("links[0].url", "invalid_url"),
        ],
        "пути полей доходят через gRPC и шлюз до клиента"
    );
    assert!(refused.request_id.is_some(), "{refused:?}");
    let shown = stack
        .web()
        .request("GET", &format!("/posts/{}", post.id.0), None, None)
        .await
        .expect("ответ");
    let public: PostView = decode(&shown.body).expect("PostView");
    assert_eq!((public.tags, public.links), (post.tags, post.links));
    stack.stop().await;
}

#[tokio::test]
async fn a_panicking_handler_is_a_plain_internal_error() {
    let routes = Routes::social().route("users.explode", |_: Ctx, _: Envelope| async {
        let empty: Vec<Reply> = Vec::new();
        Ok(empty
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("секрет из паники")))
    });
    let stack = Gateway::start_with(Config::default(), routes).await;
    let (_, mut peer) = stack.member("alice").await;
    let boom = peer.fails("users.explode", "{}").await;
    assert_eq!(boom.code, "internal");
    assert!(
        !boom.message.contains("секрет"),
        "текст паники уходит в лог, а не клиенту: {boom:?}"
    );
    assert!(
        boom.request_id.is_some(),
        "по номеру запроса найдётся запись в логе"
    );
    let alive = ok!(peer, "users.find", r#"{"username": "alice"}"# => UserView);
    assert_eq!(
        alive.username, "alice",
        "соединение пережило панику обработчика"
    );
    stack.stop().await;
}

#[tokio::test]
async fn the_course_client_shows_fields_and_codes() {
    let stack = Gateway::start(Config::default()).await;
    let mut screen = Vec::new();
    within(
        "сеанс клиента",
        run_with(
            &stack.http.to_string(),
            &stack.auth.to_string(),
            stack.dialer.clone(),
            "register ab nope\nregister alice alice@example.com\nlogin alice\npost читаю #rust про tokio https://tokio.rs\nset name Алиса Петрова\nset bio \nme\nquit\n".as_bytes(),
            &mut screen,
        ),
    )
    .await
    .expect("сеанс проходит");
    let screen = String::from_utf8(screen).expect("UTF-8");
    for needle in [
        "ошибка (validation, HTTP 422)",
        "username: too_short",
        "email: invalid_format",
        "@alice [1] читаю про tokio (лайков: 0) #rust https://tokio.rs",
        "профиль обновлён: @alice (u_1), имя «Алиса Петрова»",
        "ты @alice (u_1), имя «Алиса Петрова»",
    ] {
        assert!(screen.contains(needle), "нет «{needle}»:\n{screen}");
    }
    stack.stop().await;
}
