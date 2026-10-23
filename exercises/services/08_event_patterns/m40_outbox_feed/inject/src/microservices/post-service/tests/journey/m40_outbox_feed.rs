use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use contracts::{Fact, Origin, PostDeleted, PostEdited, Role, UserId};
use journal::Journal;
use post_service::common::{Actor, Author, Graph};
use post_service::repository::{BoxFuture, OutboxRepository};
use post_service::service::DirectoryError;
use post_service::{Draft, MemoryPosts, PostService, PostText, UserDirectory};

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

fn text(raw: &str) -> PostText {
    PostText::try_from(raw).expect("текст")
}

#[tokio::test]
async fn edits_and_deletes_are_facts_in_the_same_journal_line() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m40")
        .join("posts");
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("posts.ndjson");
    let (journal, lines) = Journal::open(Some(&path)).await.expect("журнал");
    let store = Arc::new(MemoryPosts::restore(journal, lines));
    let posts = PostService::new(store.clone());
    let origin = Origin::new("req-1");
    let author = Actor {
        id: UserId(1),
        role: Role::User,
    };
    let draft = Draft {
        text: text("черновик"),
        tags: Vec::new(),
        links: Vec::new(),
    };
    let post = posts
        .create(&Everyone, UserId(1), draft, &origin)
        .await
        .expect("пост");
    posts
        .edit(author, post.id, text("исправлено"), &origin)
        .await
        .expect("правка");
    posts
        .delete(author, post.id, &origin)
        .await
        .expect("удаление");
    let rows = store.unpublished(10).await;
    let tail: Vec<(u64, Option<Fact>)> = rows
        .iter()
        .skip(1)
        .map(|row| (row.event.version, row.event.fact().ok().flatten()))
        .collect();
    assert_eq!(
        tail,
        [
            (
                2,
                Some(Fact::PostEdited(PostEdited {
                    post_id: post.id,
                    text: String::from("исправлено")
                }))
            ),
            (3, Some(Fact::PostDeleted(PostDeleted { post_id: post.id }))),
        ],
        "правка и удаление тоже факты, версии поста идут по порядку"
    );
    let text = fs::read_to_string(&path).expect("журнал на диске");
    assert!(
        text.lines()
            .all(|line| line.contains(r#""outbox":"#) || line.contains(r#""event":"published""#)),
        "каждое изменение поста ложится в журнал вместе со своим событием:\n{text}"
    );
    let (journal, lines) = Journal::open(Some(&path)).await.expect("журнал");
    let restored = MemoryPosts::restore(journal, lines);
    assert_eq!(restored.waiting().await, 3, "outbox пережил перезапуск");
}
