// Проверка прав вынесена в контроллер, и сервис доверяет всем вызовам. Когда в соцсети появилась вторая точка
// входа (gRPC для мобильного клиента), про проверку там забыли, и любой мог править чужие посты. Решение
// должно жить в сервисе: любая дверь в него проходит через ту же проверку. Правила: заблокированный не
// может ничего; создавать посты может тот, у кого есть право; текст правит только автор (и только если у роли
// есть право `PostEditOwn`); свой пост автор удаляет по тому же праву `PostEditOwn`, а чужой удаляет только тот, у кого `PostDeleteAny`.
// Несуществующий пост это `NotFound`. Цель: `create`, `edit`, `delete` в `Posts`.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Guest,
    User,
    Moderator,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    PostCreate,
    PostEditOwn,
    PostDeleteAny,
    UserBan,
    ViewAudit,
    ManageKeys,
}

pub const ROLES: [Role; 4] = [Role::Guest, Role::User, Role::Moderator, Role::Admin];
pub const PERMISSIONS: [Permission; 6] = [
    Permission::PostCreate,
    Permission::PostEditOwn,
    Permission::PostDeleteAny,
    Permission::UserBan,
    Permission::ViewAudit,
    Permission::ManageKeys,
];

pub fn can(role: Role, permission: Permission) -> bool {
    match role {
        Role::Guest => false,
        Role::User => matches!(permission, Permission::PostCreate | Permission::PostEditOwn),
        Role::Moderator => matches!(
            permission,
            Permission::PostCreate | Permission::PostEditOwn | Permission::PostDeleteAny | Permission::UserBan
        ),
        Role::Admin => true,
    }
}

#[derive(Debug, Clone)]
pub struct Principal {
    pub user_id: u64,
    pub role: Role,
    pub banned: bool,
}

#[derive(Debug, PartialEq)]
pub enum PostError {
    NotFound,
    Forbidden,
    Banned,
}

struct Post {
    author: u64,
    text: String,
}

#[derive(Default)]
pub struct Posts {
    posts: Mutex<HashMap<u64, Post>>,
}

impl Posts {
    pub fn new() -> Posts {
        Posts::default()
    }

    pub fn text_of(&self, id: u64) -> Option<String> {
        self.posts.lock().unwrap().get(&id).map(|p| p.text.clone())
    }

    pub fn create(&self, who: &Principal, text: &str) -> Result<u64, PostError> {
        todo!()
    }

    pub fn edit(&self, who: &Principal, id: u64, text: &str) -> Result<(), PostError> {
        todo!()
    }

    pub fn delete(&self, who: &Principal, id: u64) -> Result<(), PostError> {
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn who(user_id: u64, role: Role) -> Principal {
        Principal { user_id, role, banned: false }
    }

    fn with_post() -> (Posts, u64) {
        let posts = Posts::new();
        let id = posts.create(&who(1, Role::User), "первый").unwrap();
        (posts, id)
    }

    #[test]
    fn creating_needs_the_permission() {
        let posts = Posts::new();
        assert!(posts.create(&who(1, Role::User), "привет").is_ok(), "пользователь пишет");
        assert_eq!(posts.create(&who(2, Role::Guest), "привет"), Err(PostError::Forbidden), "гость нет");
    }

    #[test]
    fn only_the_author_edits_text() {
        let (posts, id) = with_post();
        assert_eq!(posts.edit(&who(2, Role::User), id, "взлом"), Err(PostError::Forbidden), "чужой пользователь");
        assert_eq!(posts.edit(&who(9, Role::Moderator), id, "взлом"), Err(PostError::Forbidden), "модератор чужой текст не правит");
        assert_eq!(posts.edit(&who(9, Role::Admin), id, "взлом"), Err(PostError::Forbidden), "и админ тоже: правка текста это право автора");
        assert_eq!(posts.text_of(id).as_deref(), Some("первый"), "текст не изменился");
        assert_eq!(posts.edit(&who(1, Role::User), id, "правка"), Ok(()), "автор правит");
        assert_eq!(posts.text_of(id).as_deref(), Some("правка"), "текст изменился");
    }

    #[test]
    fn delete_belongs_to_author_or_moderation() {
        let (posts, id) = with_post();
        assert_eq!(posts.delete(&who(2, Role::User), id), Err(PostError::Forbidden), "чужой пользователь");
        assert_eq!(posts.delete(&who(9, Role::Guest), id), Err(PostError::Forbidden), "гость");
        assert!(posts.text_of(id).is_some(), "пост на месте");
        assert_eq!(posts.delete(&who(9, Role::Moderator), id), Ok(()), "модератор удаляет чужое");
        assert_eq!(posts.text_of(id), None, "пост удалён");
        let id2 = posts.create(&who(1, Role::User), "ещё").unwrap();
        assert_eq!(posts.delete(&who(1, Role::User), id2), Ok(()), "автор удаляет своё");
    }

    #[test]
    fn missing_post_is_not_found() {
        let posts = Posts::new();
        assert_eq!(posts.edit(&who(1, Role::User), 77, "x"), Err(PostError::NotFound), "правка");
        assert_eq!(posts.delete(&who(1, Role::Admin), 77), Err(PostError::NotFound), "удаление");
    }

    #[test]
    fn banned_user_can_do_nothing_even_with_a_strong_role() {
        let (posts, id) = with_post();
        for role in [Role::User, Role::Moderator, Role::Admin] {
            let banned = Principal { user_id: 1, role, banned: true };
            assert_eq!(posts.create(&banned, "x"), Err(PostError::Banned), "{role:?}: создание");
            assert_eq!(posts.edit(&banned, id, "x"), Err(PostError::Banned), "{role:?}: правка своего поста");
            assert_eq!(posts.delete(&banned, id), Err(PostError::Banned), "{role:?}: удаление");
        }
        assert_eq!(posts.delete(&Principal { user_id: 5, role: Role::User, banned: true }, 404), Err(PostError::Banned), "бан важнее, чем отсутствие поста");
    }

    #[test]
    fn author_who_lost_the_right_cannot_edit() {
        let (posts, id) = with_post();
        let demoted = who(1, Role::Guest);
        assert_eq!(posts.edit(&demoted, id, "x"), Err(PostError::Forbidden), "автор, лишённый роли, не правит и свои посты");
    }

    #[test]
    fn author_who_lost_the_right_cannot_delete_either() {
        let (posts, id) = with_post();
        let demoted = who(1, Role::Guest);
        assert_eq!(posts.delete(&demoted, id), Err(PostError::Forbidden), "автор без права PostEditOwn не удаляет и свой пост");
    }
}
