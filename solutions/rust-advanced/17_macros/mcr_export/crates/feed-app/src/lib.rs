use social_macros::{require, valid_login, Invalid};

#[derive(Debug, PartialEq)]
pub struct User {
    pub login: String,
    pub age: u8,
}

pub fn register(login: &str, age: u8) -> Result<User, Invalid> {
    let login = valid_login!(login)?;
    require!(age >= 13, "age", "нужно минимум 13 лет");
    Ok(User { login, age })
}

pub fn rename(old: &str, new: &str) -> Result<String, Invalid> {
    social_macros::require!(old != new, "login", "новое имя совпадает со старым");
    valid_login!(new)
}

pub mod shadowing {
    #[derive(Debug)]
    pub struct Invalid;

    pub fn nickname_ok(nick: &str) -> Result<usize, social_macros::Invalid> {
        let nick = social_macros::valid_login!(nick)?;
        Ok(nick.len())
    }

    pub fn unused() -> Invalid {
        Invalid
    }
}
