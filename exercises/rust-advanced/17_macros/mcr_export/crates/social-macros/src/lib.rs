mod errors {
    #[derive(Debug, PartialEq, Clone)]
    pub struct Invalid {
        pub field: &'static str,
        pub reason: String,
    }
}

mod validate {
    pub fn is_valid_login(login: &str) -> bool {
        !login.is_empty() && login.len() <= 20 && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }
}

pub use errors::Invalid;

/// Быстрая проверка в самом крейте.
pub fn quick_check(login: &str) -> Result<(), Invalid> {
    require!(!login.is_empty(), "login", "пустой логин");
    Ok(())
}

macro_rules! require {
    ($cond:expr, $field:expr, $reason:expr) => {
        if !$cond {
            return Err(Invalid { field: $field, reason: $reason.to_string() });
        }
    };
}

macro_rules! valid_login {
    ($login:expr) => {{
        let login: String = $login.to_string();
        (|| -> Result<String, Invalid> {
            require!(validate::is_valid_login(&login), "login", format!("недопустимый логин {login:?}"));
            Ok(login.clone())
        })()
    }};
}
