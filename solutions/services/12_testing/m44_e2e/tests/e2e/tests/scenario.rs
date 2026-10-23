use contracts::{Changed, FeedPage, PostView, Push, UserView, decode};
use e2e::Stack;
use e2e::eventually;
use test_support::{post, user};
use tokio::sync::Mutex;

#[tokio::test]
async fn register_login_follow_post_and_the_follower_hears_it() {
    let stack = Stack::start().await;
    let alice = user().named("alice");
    let bob = user().named("bob");
    let (alice_view, mut writer) = stack.member(&alice).await;
    let (bob_view, mut reader) = stack.member(&bob).await;
    assert_eq!(alice_view.username, alice.username);
    let followed: Changed = reader
        .ok(
            "users.follow",
            &format!(r#"{{"target": "{}"}}"#, alice_view.id),
        )
        .await;
    assert!(followed.changed);
    assert_eq!(
        writer.push().await,
        Push::Followed {
            to: alice_view.id,
            by: bob_view.id
        },
        "автор узнаёт о подписчике"
    );
    let text = post().text("привет из e2e");
    let created: PostView = writer.ok("posts.create", &text.body()).await;
    assert_eq!(created.text, "привет из e2e");
    let heard = reader
        .push_where(|push| matches!(push, Push::NewPost { .. }))
        .await;
    assert_eq!(
        heard,
        Push::NewPost {
            to: bob_view.id,
            author: alice_view.id,
            post: created.id
        },
        "подписчик слышит о новом посте"
    );
    let token = stack.login(&bob).await.token;
    let again = Mutex::new(stack.connect(&token).await);
    let page: FeedPage = eventually("пост alice в ленте bob", || {
        let again = &again;
        async move {
            let page: FeedPage = again.lock().await.ok("posts.feed", "{}").await;
            if page.posts.iter().any(|post| post.id == created.id) {
                Ok(page)
            } else {
                Err(format!("в ленте {} постов", page.posts.len()))
            }
        }
    })
    .await;
    assert_eq!(page.posts[0].author, alice_view.id);
    stack.stop().await;
}

#[tokio::test]
async fn profile_patch_is_visible_over_http() {
    let stack = Stack::start().await;
    let carol = user().named("carol").display_name("Кэрол").bio("пишу e2e");
    stack.register(&carol).await;
    let token = stack.login(&carol).await.token;
    let patch = carol.profile_patch().expect("есть что поменять");
    let patched = stack
        .request("PATCH", "/users/me", Some(&patch), Some(&token))
        .await;
    assert_eq!(patched.status, 200, "{patched:?}");
    let found = eventually("профиль обновлён", || async {
        let reply = stack
            .request("GET", &format!("/users/{}", carol.username), None, None)
            .await;
        let view: UserView = decode(&reply.body).map_err(|error| error.to_string())?;
        match view.display_name.as_deref() {
            Some("Кэрол") => Ok(view),
            other => Err(format!("display_name {other:?}")),
        }
    })
    .await;
    assert_eq!(found.bio.as_deref(), Some("пишу e2e"));
    stack.stop().await;
}
