
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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn granted(role: Role) -> Vec<Permission> {
        PERMISSIONS.iter().copied().filter(|p| can(role, *p)).collect()
    }

    #[test]
    fn guest_has_nothing() {
        assert_eq!(granted(Role::Guest), vec![], "гость ничего не может");
    }

    #[test]
    fn user_writes_and_edits_own_posts() {
        assert_eq!(granted(Role::User), vec![Permission::PostCreate, Permission::PostEditOwn], "права пользователя");
    }

    #[test]
    fn moderator_adds_deleting_and_banning() {
        assert_eq!(
            granted(Role::Moderator),
            vec![Permission::PostCreate, Permission::PostEditOwn, Permission::PostDeleteAny, Permission::UserBan],
            "права модератора"
        );
        assert!(!can(Role::Moderator, Permission::ManageKeys), "ключами модератор не управляет");
        assert!(!can(Role::Moderator, Permission::ViewAudit), "аудит ему тоже не нужен");
    }

    #[test]
    fn admin_has_everything_listed() {
        assert_eq!(granted(Role::Admin).len(), PERMISSIONS.len(), "админ может всё, что есть в списке прав");
    }

    #[test]
    fn rights_only_grow_with_the_role() {
        for pair in ROLES.windows(2) {
            for permission in PERMISSIONS {
                assert!(
                    !can(pair[0], permission) || can(pair[1], permission),
                    "{:?} имеет {permission:?}, а старшая роль {:?} нет: роли образуют лестницу",
                    pair[0],
                    pair[1]
                );
            }
        }
    }
}
