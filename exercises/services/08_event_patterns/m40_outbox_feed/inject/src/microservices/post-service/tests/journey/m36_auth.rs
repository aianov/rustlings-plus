use std::sync::Arc;

use contracts::{Origin, Permission, Role, UserId};
use post_service::common::{Actor, Author, Graph};
use post_service::repository::BoxFuture;
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

fn text(raw: &str) -> PostText {
    PostText::try_from(raw).expect("текст")
}

fn draft(raw: &str) -> Draft {
    Draft {
        text: text(raw),
        tags: Vec::new(),
        links: Vec::new(),
    }
}

fn actor(id: u64, role: Role) -> Actor {
    Actor {
        id: UserId(id),
        role,
    }
}

#[test]
fn the_role_table_denies_by_default() {
    assert!(!Role::Guest.allows(Permission::Write), "гость без прав");
    assert!(
        Role::User.allows(Permission::EditOwnPost) && !Role::User.allows(Permission::DeleteAnyPost)
    );
    assert!(Role::Moderator.allows(Permission::DeleteAnyPost));
    assert_eq!(Role::parse("admin"), Role::Admin);
    for strange in ["Admin", "root", "", " user"] {
        assert_eq!(
            Role::parse(strange),
            Role::Guest,
            "{strange:?}: незнакомая роль это гость, а не администратор"
        );
    }
}

#[tokio::test]
async fn authors_edit_and_moderators_clean_up() {
    let posts = Arc::new(PostService::new(Arc::new(MemoryPosts::new())));
    let mine = posts
        .create(&Everyone, UserId(1), draft("черновик"), &Origin::new("t"))
        .await
        .expect("пост")
        .id;
    let edited = posts
        .edit(
            actor(1, Role::User),
            mine,
            text("исправлено"),
            &Origin::new("t"),
        )
        .await
        .expect("автор правит свой пост");
    assert_eq!(edited.text, "исправлено");
    for stranger in [actor(2, Role::User), actor(3, Role::Moderator)] {
        let refused = posts
            .edit(stranger, mine, text("чужое"), &Origin::new("t"))
            .await
            .expect_err("чужой текст не правят");
        assert!(matches!(refused, PostError::NotAuthor(..)), "{refused}");
        assert_eq!(refused.code(), "forbidden");
    }
    assert!(
        posts
            .delete(actor(2, Role::User), mine, &Origin::new("t"))
            .await
            .is_err(),
        "обычный пользователь чужое не удаляет"
    );
    assert!(
        posts
            .delete(actor(1, Role::Guest), mine, &Origin::new("t"))
            .await
            .is_err(),
        "без роли нет и прав на своё: запрет по умолчанию"
    );
    assert!(
        posts
            .delete(actor(3, Role::Moderator), mine, &Origin::new("t"))
            .await
            .expect("модератор"),
        "модератор удаляет чужой пост"
    );
    let own = posts
        .create(&Everyone, UserId(2), draft("моё"), &Origin::new("t"))
        .await
        .expect("пост")
        .id;
    assert!(
        posts
            .delete(UserId(2), own, &Origin::new("t"))
            .await
            .expect("автор"),
        "автор удаляет своё"
    );
}
