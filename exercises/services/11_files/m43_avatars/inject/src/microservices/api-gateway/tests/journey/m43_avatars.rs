use std::time::Duration;

use api_gateway::Config;
use contracts::{UserView, decode};
use social_cli::{Body, Reply};

use crate::support::{Gateway, within};

fn png(width: u32, height: u32, size: usize) -> Vec<u8> {
    let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    let head = data.len();
    data.extend((head..size.max(head)).map(|i| (i * 13 % 251) as u8));
    data
}

async fn put(stack: &Gateway, token: &str, content_type: &str, bytes: &[u8]) -> Reply {
    let bearer = format!("Bearer {token}");
    within(
        "загрузка аватара",
        stack.web().send(
            "PUT",
            "/users/me/avatar",
            Some(Body {
                content_type,
                bytes,
            }),
            &[("Authorization", &bearer)],
        ),
    )
    .await
    .expect("шлюз отвечает")
}

async fn get(stack: &Gateway, path: &str, headers: &[(&str, &str)]) -> Reply {
    within("скачивание", stack.web().send("GET", path, None, headers))
        .await
        .expect("шлюз отвечает")
}

async fn profile(stack: &Gateway, name: &str) -> UserView {
    let answer = stack.http("GET", &format!("/users/{name}"), None).await;
    assert_eq!(answer.status, 200, "{answer:?}");
    decode(&answer.body).expect("профиль разбирается")
}

#[tokio::test]
async fn avatar_goes_through_a_signed_link() {
    let stack = Gateway::start(Config::default()).await;
    stack.register("alice").await;
    let login = stack.login("alice").await;
    assert_eq!(
        profile(&stack, "alice").await.avatar,
        None,
        "без аватара поля нет"
    );
    let picture = png(48, 48, 40_000);
    let stored = put(&stack, &login.token, "application/octet-stream", &picture).await;
    assert_eq!(
        stored.status, 201,
        "тип решает сигнатура, а не Content-Type: {stored:?}"
    );
    let location = stored
        .header("location")
        .expect("ответ несёт ссылку")
        .to_string();
    let link = profile(&stack, "alice")
        .await
        .avatar
        .expect("профиль отдаёт подписанную ссылку");
    assert!(
        link.starts_with("/files/") && link.contains("&sig="),
        "{link}"
    );
    assert_eq!(
        link.split('?').next(),
        location.split('?').next(),
        "ссылка из профиля и из ответа ведут к одному ключу"
    );
    let full = get(&stack, &link, &[]).await;
    assert_eq!((full.status, &full.bytes), (200, &picture));
    assert_eq!(full.header("content-type"), Some("image/png"));
    assert_eq!(full.header("x-content-type-options"), Some("nosniff"));
    assert!(
        full.header("content-disposition")
            .is_some_and(|value| value.contains("filename=\"avatar.png\"")),
        "{:?}",
        full.header("content-disposition")
    );
    let tail = get(&stack, &link, &[("Range", "bytes=39990-")]).await;
    assert_eq!(
        (tail.status, &tail.bytes[..]),
        (206, &picture[39_990..]),
        "Range через шлюз"
    );
    let sig_at = link.find("sig=").expect("подпись") + 4;
    let mut forged = link.clone().into_bytes();
    forged[sig_at] = if forged[sig_at] == b'0' { b'1' } else { b'0' };
    let forged = String::from_utf8(forged).expect("ascii");
    assert_eq!(get(&stack, &forged, &[]).await.status, 403);
    let renamed = link.replace("name=avatar.png", "name=avatar.html");
    assert_eq!(
        get(&stack, &renamed, &[]).await.status,
        403,
        "имя файла тоже под подписью"
    );
    assert_eq!(get(&stack, "/files/abc", &[]).await.status, 400);
    let text = put(&stack, &login.token, "image/png", b"<svg onload=alert(1)>").await;
    assert_eq!(text.status, 415, "SVG и текст не картинка: {text:?}");
    let anonymous = within(
        "загрузка без токена",
        stack.web().send(
            "PUT",
            "/users/me/avatar",
            Some(Body {
                content_type: "image/png",
                bytes: &picture,
            }),
            &[],
        ),
    )
    .await
    .expect("шлюз отвечает");
    assert_eq!(anonymous.status, 401);
    stack.stop().await;
}

#[tokio::test]
async fn expired_link_does_not_open() {
    let stack = Gateway::start(Config {
        link_ttl: Duration::from_secs(1),
        ..Config::default()
    })
    .await;
    stack.register("bob").await;
    let login = stack.login("bob").await;
    let stored = put(&stack, &login.token, "image/png", &png(8, 8, 500)).await;
    assert_eq!(stored.status, 201, "{stored:?}");
    let link = profile(&stack, "bob").await.avatar.expect("ссылка");
    assert_eq!(
        get(&stack, &link, &[]).await.status,
        200,
        "свежая открывается"
    );
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    let late = get(&stack, &link, &[]).await;
    assert_eq!(late.status, 410, "просроченная ссылка: 410 Gone: {late:?}");
    let fresh = profile(&stack, "bob").await.avatar.expect("новая ссылка");
    assert_ne!(fresh, link, "профиль выдаёт новую ссылку с новым сроком");
    assert_eq!(get(&stack, &fresh, &[]).await.status, 200);
    stack.stop().await;
}

#[tokio::test]
async fn avatar_limit_is_checked_before_the_upstream() {
    let stack = Gateway::start(Config {
        avatar_max: 1_000,
        ..Config::default()
    })
    .await;
    stack.register("carol").await;
    let login = stack.login("carol").await;
    let big = put(&stack, &login.token, "image/png", &png(8, 8, 5_000)).await;
    assert_eq!(big.status, 413);
    let fits = put(&stack, &login.token, "image/png", &png(8, 8, 900)).await;
    assert_eq!(fits.status, 201, "{fits:?}");
    let again = put(&stack, &login.token, "image/png", &png(8, 8, 900)).await;
    assert_eq!(again.status, 204, "та же картинка: замена без нового файла");
    stack.stop().await;
}
