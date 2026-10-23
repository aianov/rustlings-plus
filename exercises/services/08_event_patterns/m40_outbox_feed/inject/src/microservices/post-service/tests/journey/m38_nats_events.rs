use std::sync::Arc;

use contracts::{Fact, Origin, PostCommented, PostCreated, PostId, PostLiked, UserId};
use post_service::common::{Author, CommentText, Graph};
use post_service::repository::{BoxFuture, OutboxRepository};
use post_service::service::DirectoryError;
use post_service::{Draft, MemoryPosts, PostError, PostService, PostText, UserDirectory};

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

struct Nobody;

impl UserDirectory for Nobody {
    fn author(&self, _: UserId) -> BoxFuture<'_, Result<Option<Author>, DirectoryError>> {
        Box::pin(async { Ok(None) })
    }

    fn graph(&self, _: UserId) -> BoxFuture<'_, Result<Option<Graph>, DirectoryError>> {
        Box::pin(async { Ok(None) })
    }
}

fn draft(raw: &str) -> Draft {
    Draft {
        text: PostText::try_from(raw).expect("текст"),
        tags: Vec::new(),
        links: Vec::new(),
    }
}

fn comment(raw: &str) -> CommentText {
    CommentText::try_from(raw).expect("комментарий")
}

async fn facts(posts: &MemoryPosts) -> Vec<Fact> {
    posts
        .unpublished(100)
        .await
        .into_iter()
        .map(|row| {
            assert_eq!(
                row.request_id, "e856e880e9a85e6d",
                "у события номер запроса"
            );
            row.event.fact().ok().flatten().expect("свой факт читается")
        })
        .collect()
}

#[tokio::test]
async fn every_change_of_a_post_becomes_a_fact_after_it_happened() {
    let store = Arc::new(MemoryPosts::new());
    let posts = PostService::new(store.clone());
    let origin = Origin::new("e856e880e9a85e6d");
    let (alice, bob, carol) = (UserId(1), UserId(2), UserId(3));
    let post = posts
        .create(&Everyone, alice, draft("привет"), &origin)
        .await
        .expect("пост");
    assert!(
        posts
            .like(bob, post.id, &origin)
            .await
            .expect("лайк")
            .changed
    );
    assert!(
        !posts
            .like(bob, post.id, &origin)
            .await
            .expect("повтор")
            .changed
    );
    assert!(
        posts
            .like(alice, post.id, &origin)
            .await
            .expect("свой")
            .changed
    );
    posts
        .comment(carol, post.id, comment("согласна"), &origin)
        .await
        .expect("комментарий");
    assert_eq!(
        facts(&store).await,
        [
            Fact::PostCreated(PostCreated {
                post_id: post.id,
                author_id: alice,
                author_name: String::from("user1"),
                text: String::from("привет"),
                tags: Vec::new(),
                links: Vec::new(),
            }),
            Fact::PostLiked(PostLiked {
                post_id: post.id,
                author_id: alice,
                user_id: bob
            }),
            Fact::PostLiked(PostLiked {
                post_id: post.id,
                author_id: alice,
                user_id: alice
            }),
            Fact::PostCommented(PostCommented {
                post_id: post.id,
                author_id: alice,
                comment_id: contracts::CommentId(1),
                commenter_id: carol
            }),
        ],
        "событие это факт: создан, лайкнут (и своим лайком тоже), прокомментирован; \
         повторный лайк ничего не изменил и события не дал"
    );
}

#[tokio::test]
async fn a_refused_change_publishes_nothing() {
    let store = Arc::new(MemoryPosts::new());
    let posts = PostService::new(store.clone());
    let origin = Origin::new("e856e880e9a85e6d");
    assert!(matches!(
        posts
            .create(&Nobody, UserId(9), draft("кто я"), &origin)
            .await,
        Err(PostError::UnknownAuthor(_))
    ));
    assert!(matches!(
        posts.like(UserId(2), PostId(77), &origin).await,
        Err(PostError::UnknownPost(_))
    ));
    assert!(matches!(
        posts
            .comment(UserId(2), PostId(77), comment("эй"), &origin)
            .await,
        Err(PostError::UnknownPost(_))
    ));
    assert!(
        store.unpublished(100).await.is_empty(),
        "отказ ничего не меняет и событий не даёт"
    );
}
