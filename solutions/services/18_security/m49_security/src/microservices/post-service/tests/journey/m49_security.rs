use std::net::SocketAddr;
use std::sync::Arc;

use contracts::{CommentId, Origin, Role, UserId};
use http_client::AddrGuard;
use post_service::common::{Actor, Author, CommentText, Graph};
use post_service::repository::BoxFuture;
use post_service::service::DirectoryError;
use post_service::{
    Draft, MemoryPosts, PostError, PostService, PostText, Previewer, UserDirectory, extract_title,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct Everyone;

impl UserDirectory for Everyone {
    fn author(&self, id: UserId) -> BoxFuture<'_, Result<Option<Author>, DirectoryError>> {
        Box::pin(async move {
            Ok(Some(Author {
                id,
                name: format!("user{}", id.0),
            }))
        })
    }

    fn graph(&self, _: UserId) -> BoxFuture<'_, Result<Option<Graph>, DirectoryError>> {
        Box::pin(async { Ok(Some(Graph::default())) })
    }
}

async fn serve(reply: String) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let reply = reply.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let _ = socket.write_all(reply.as_bytes()).await;
            });
        }
    });
    addr
}

fn html(body: &str, kind: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn only(port: u16) -> Previewer {
    Previewer::with_guard(AddrGuard::new(move |addr| addr.port() == port))
}

#[test]
fn titles_are_cleaned() {
    assert_eq!(
        extract_title("<html><TITLE lang=ru>\n  Привет &amp; мир  </TITLE></html>").as_deref(),
        Some("Привет & мир")
    );
    assert_eq!(extract_title("<title></title>"), None);
    assert_eq!(extract_title("без заголовка"), None);
    let long = format!("<title>{}</title>", "я".repeat(500));
    assert_eq!(extract_title(&long).map(|t| t.chars().count()), Some(200));
}

#[tokio::test]
async fn a_public_page_gets_a_preview() {
    let addr = serve(html("<title>Страница</title>", "text/html; charset=utf-8")).await;
    let preview = only(addr.port());
    assert_eq!(
        preview.title(&format!("http://{addr}/")).await.as_deref(),
        Some("Страница")
    );
    let json = serve(html("{\"title\": 1}", "application/json")).await;
    assert_eq!(
        only(json.port()).title(&format!("http://{json}/")).await,
        None
    );
    for scheme in ["file:///etc/passwd", "ftp://example.com/", "gopher://x/"] {
        assert_eq!(preview.title(scheme).await, None, "{scheme}");
    }
}

#[tokio::test]
async fn internal_addresses_get_no_preview() {
    let addr = serve(html("<title>секрет</title>", "text/html")).await;
    let preview = Previewer::new();
    assert_eq!(
        preview.title(&format!("http://{addr}/")).await,
        None,
        "loopback закрыт"
    );
    assert_eq!(
        preview
            .title(&format!("http://localhost:{}/", addr.port()))
            .await,
        None
    );
    let outer = serve(format!(
        "HTTP/1.1 301 Moved\r\nLocation: http://{addr}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    ))
    .await;
    assert_eq!(
        only(outer.port()).title(&format!("http://{outer}/")).await,
        None,
        "редирект на внутренний адрес проверяется заново"
    );
    let huge = serve(html(
        &format!("<title>большой</title>{}", "x".repeat(200_000)),
        "text/html",
    ))
    .await;
    assert_eq!(
        only(huge.port()).title(&format!("http://{huge}/")).await,
        None,
        "лимит размера"
    );
}

fn service() -> PostService {
    let store = Arc::new(MemoryPosts::new());
    PostService::new(store.clone()).with_drafts(store)
}

#[tokio::test]
async fn only_owners_touch_comments_and_drafts() {
    let posts = service();
    let origin = Origin::new("m49");
    let (alice, bob, eve) = (UserId(1), UserId(2), UserId(3));
    let draft = Draft {
        text: PostText::try_from("пост").expect("текст"),
        tags: Vec::new(),
        links: Vec::new(),
    };
    let post = posts
        .create(&Everyone, alice, draft, &origin)
        .await
        .expect("пост")
        .id;
    let first = posts
        .comment(
            bob,
            post,
            CommentText::try_from("раз").expect("текст"),
            &origin,
        )
        .await
        .expect("комментарий");
    let second = posts
        .comment(
            bob,
            post,
            CommentText::try_from("два").expect("текст"),
            &origin,
        )
        .await
        .expect("комментарий");
    let user = |id| Actor {
        id,
        role: Role::User,
    };
    assert!(matches!(
        posts.delete_comment(user(eve), first, &origin).await,
        Err(PostError::NotCommentOwner(..))
    ));
    assert!(
        posts
            .delete_comment(user(bob), first, &origin)
            .await
            .expect("автор")
    );
    assert!(
        posts
            .delete_comment(user(alice), second, &origin)
            .await
            .expect("автор поста")
    );
    assert!(matches!(
        posts.delete_comment(user(bob), first, &origin).await,
        Err(PostError::UnknownComment(..))
    ));
    assert!(matches!(
        posts
            .delete_comment(user(bob), CommentId(99), &origin)
            .await,
        Err(PostError::UnknownComment(..))
    ));
    let kept = posts
        .save_draft(
            alice,
            PostText::try_from("черновик").expect("текст"),
            &origin,
        )
        .await
        .expect("черновик");
    assert_eq!(posts.draft(alice, kept).await.expect("свой"), "черновик");
    assert!(
        matches!(
            posts.draft(eve, kept).await,
            Err(PostError::UnknownDraft(..))
        ),
        "чужой черновик выглядит как несуществующий"
    );
    assert!(matches!(
        posts.delete_draft(eve, kept, &origin).await,
        Err(PostError::UnknownDraft(..))
    ));
    assert!(
        posts
            .delete_draft(alice, kept, &origin)
            .await
            .expect("свой")
    );
    assert!(matches!(
        posts.draft(alice, kept).await,
        Err(PostError::UnknownDraft(..))
    ));
}
