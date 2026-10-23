use std::sync::Arc;

use contracts::{Role, UserId};
use post_service::common::{Actor, Author, Graph, MAX_AUTHOR_POSTS};
use post_service::dto::author_page;
use post_service::repository::BoxFuture;
use post_service::service::DirectoryError;
use post_service::{Draft, MemoryPosts, PostService, PostText, UserDirectory};
use proto::social::v1::AuthorPostsRequest;

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

fn draft(raw: &str) -> Draft {
    Draft {
        text: PostText::try_from(raw).expect("текст"),
        tags: Vec::new(),
        links: Vec::new(),
    }
}

#[tokio::test]
async fn author_posts_are_the_newest_of_one_author() {
    let posts = Arc::new(PostService::new(Arc::new(MemoryPosts::new())));
    for (author, text) in [
        (1, "старый"),
        (2, "чужой"),
        (1, "средний"),
        (1, "удалю"),
        (1, "новый"),
    ] {
        posts
            .create(&Everyone, UserId(author), draft(text))
            .await
            .expect("пост");
    }
    let doomed = posts
        .by_author(UserId(1), 1)
        .await
        .first()
        .map(|post| post.id)
        .expect("последний пост есть");
    assert!(
        posts
            .delete(
                Actor {
                    id: UserId(1),
                    role: Role::User
                },
                doomed
            )
            .await
            .expect("удаляется")
    );
    let texts: Vec<String> = posts
        .by_author(UserId(1), 2)
        .await
        .into_iter()
        .map(|post| post.text.to_string())
        .collect();
    assert_eq!(
        texts,
        ["удалю", "средний"],
        "посты одного автора, новые первыми, удалённых нет, не больше limit"
    );
}

#[test]
fn the_page_size_is_bounded() {
    let ask = |limit| AuthorPostsRequest {
        author_id: 1,
        limit,
    };
    assert_eq!(author_page(&ask(10)).ok(), Some((UserId(1), 10)));
    for limit in [0, MAX_AUTHOR_POSTS as u32 + 1] {
        let refused = author_page(&ask(limit)).expect_err("limit вне границ");
        assert_eq!(
            refused.pairs(),
            vec![("limit", "out_of_range")],
            "limit от 1 до {MAX_AUTHOR_POSTS}: шлюз не вытащит всю историю автора одним вызовом"
        );
    }
}
