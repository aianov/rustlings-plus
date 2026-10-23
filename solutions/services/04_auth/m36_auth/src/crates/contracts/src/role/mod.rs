use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Guest,
    User,
    Moderator,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    Write,
    EditOwnPost,
    DeleteOwnPost,
    DeleteAnyPost,
}

const GRANTS: &[(Role, Permission)] = &[
    (Role::User, Permission::Write),
    (Role::User, Permission::EditOwnPost),
    (Role::User, Permission::DeleteOwnPost),
    (Role::Moderator, Permission::Write),
    (Role::Moderator, Permission::EditOwnPost),
    (Role::Moderator, Permission::DeleteOwnPost),
    (Role::Moderator, Permission::DeleteAnyPost),
    (Role::Admin, Permission::Write),
    (Role::Admin, Permission::EditOwnPost),
    (Role::Admin, Permission::DeleteOwnPost),
    (Role::Admin, Permission::DeleteAnyPost),
];

impl Role {
    pub const ALL: [Role; 4] = [Role::Guest, Role::User, Role::Moderator, Role::Admin];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Guest => "guest",
            Role::User => "user",
            Role::Moderator => "moderator",
            Role::Admin => "admin",
        }
    }

    pub fn parse(text: &str) -> Role {
        match text {
            "user" => Role::User,
            "moderator" => Role::Moderator,
            "admin" => Role::Admin,
            _unknown => Role::Guest,
        }
    }

    pub fn allows(self, permission: Permission) -> bool {
        GRANTS.contains(&(self, permission))
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
