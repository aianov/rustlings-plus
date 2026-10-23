
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

pub fn role_from_claim(claim: Option<&str>) -> Role {
    match claim {
        Some("user") => Role::User,
        Some("moderator") => Role::Moderator,
        Some("admin") => Role::Admin,
        _ => Role::Guest,
    }
}

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

pub fn allowed(claim: Option<&str>, permission: Permission) -> bool {
    can(role_from_claim(claim), permission)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_roles_are_recognized() {
        assert_eq!(role_from_claim(Some("user")), Role::User, "user");
        assert_eq!(role_from_claim(Some("moderator")), Role::Moderator, "moderator");
        assert_eq!(role_from_claim(Some("admin")), Role::Admin, "admin");
    }

    #[test]
    fn everything_else_is_a_guest() {
        for claim in [None, Some(""), Some("superuser"), Some("root"), Some("administrator"), Some("null"), Some("0")] {
            assert_eq!(role_from_claim(claim), Role::Guest, "{claim:?} это не роль из нашего списка, права гостя");
        }
    }

    #[test]
    fn spelling_must_be_exact() {
        for claim in ["ADMIN", "Admin", " admin", "admin ", "admin\n", "adm\u{0456}n"] {
            assert_eq!(role_from_claim(Some(claim)), Role::Guest, "{claim:?} похоже на роль, но не она: токен подписываем мы, и мы пишем точно");
        }
    }

    #[test]
    fn unknown_role_cannot_do_admin_things() {
        for claim in [None, Some("superuser"), Some("")] {
            assert!(!allowed(claim, Permission::ManageKeys), "{claim:?} получил управление ключами");
            assert!(!allowed(claim, Permission::PostCreate), "{claim:?} получил даже право писать");
        }
        assert!(allowed(Some("admin"), Permission::ManageKeys), "настоящий админ управляет ключами");
    }
}
