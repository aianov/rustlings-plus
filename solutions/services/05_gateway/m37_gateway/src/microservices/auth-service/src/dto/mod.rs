use contracts::{Email, MAX_NAME_LEN, MIN_NEW_NAME_LEN, UserId, UserView, Username};
use proto::social::v1::Account as AccountReply;
use serde::Deserialize;
use service_runtime::Secret;
use validation::{CharLen, Invalid, NotBlank, RuleExt, Validator, Violation};

use crate::common::{Account, Registration};

pub const MIN_PASSWORD: usize = 8;

pub const MAX_PASSWORD: usize = 128;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrationBody {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordChangeBody {
    pub old_password: String,
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct ForgotBody {
    pub username: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetBody {
    pub token: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct ConfirmBody {
    pub token: String,
}

fn password(check: &mut Validator, field: &str, raw: &str, name: Option<&str>) -> Secret<String> {
    let rule = NotBlank.and(CharLen {
        min: MIN_PASSWORD,
        max: MAX_PASSWORD,
    });
    if check.check(field, raw, &rule) && name.is_some_and(|name| raw.eq_ignore_ascii_case(name)) {
        check.report(
            field,
            Violation::new("same_as_username", "пароль совпадает с ником"),
        );
    }
    Secret::new(raw.to_string())
}

pub fn registration(body: &RegistrationBody) -> Result<Registration, Invalid> {
    let mut check = Validator::new();
    let username = check.parse::<Username>("username", &body.username);
    if let Some(name) = &username {
        let length = CharLen {
            min: MIN_NEW_NAME_LEN,
            max: MAX_NAME_LEN,
        };
        check.check("username", name.as_ref(), &length);
    }
    let name = username.as_ref().map(AsRef::as_ref);
    let password = password(&mut check, "password", &body.password, name);
    let email = body
        .email
        .as_deref()
        .and_then(|raw| check.parse::<Email>("email", raw));
    let username = check.finish_with(username)?;
    Ok(Registration {
        username,
        email,
        password,
    })
}

pub fn login(body: &LoginBody) -> Result<(Username, Secret<String>), Invalid> {
    let mut check = Validator::new();
    let username = check.parse::<Username>("username", &body.username);
    check.check("password", body.password.as_str(), &NotBlank);
    let username = check.finish_with(username)?;
    Ok((username, Secret::new(body.password.clone())))
}

pub fn password_change(
    body: &PasswordChangeBody,
) -> Result<(Secret<String>, Secret<String>), Invalid> {
    let mut check = Validator::new();
    check.check("old_password", body.old_password.as_str(), &NotBlank);
    let new = password(&mut check, "new_password", &body.new_password, None);
    if body.new_password == body.old_password {
        check.report(
            "new_password",
            Violation::new("same_as_old", "новый пароль совпадает со старым"),
        );
    }
    check.finish()?;
    Ok((Secret::new(body.old_password.clone()), new))
}

pub fn forgot(body: &ForgotBody) -> Result<Username, Invalid> {
    let mut check = Validator::new();
    let username = check.parse::<Username>("username", &body.username);
    check.finish_with(username)
}

pub fn reset(body: &ResetBody) -> Result<(String, Secret<String>), Invalid> {
    let mut check = Validator::new();
    check.check("token", body.token.as_str(), &NotBlank);
    let password = password(&mut check, "password", &body.password, None);
    check.finish()?;
    Ok((body.token.clone(), password))
}

pub fn view(account: &Account) -> UserView {
    UserView::named(UserId(account.user_id), &account.username)
}

pub fn reply(account: Account) -> AccountReply {
    AccountReply {
        user_id: account.user_id,
        role: account.role().as_str().to_string(),
        username: account.username,
        expires_at: 0,
    }
}
