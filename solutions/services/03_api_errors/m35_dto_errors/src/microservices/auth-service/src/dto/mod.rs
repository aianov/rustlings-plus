use contracts::{Email, MAX_NAME_LEN, MIN_NEW_NAME_LEN, UserId, UserView, Username};
use proto::social::v1::Account as AccountReply;
use serde::Deserialize;
use validation::{CharLen, Invalid, Validator};

use crate::common::{Account, Registration};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrationBody {
    pub username: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub username: String,
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
    let email = body
        .email
        .as_deref()
        .and_then(|raw| check.parse::<Email>("email", raw));
    let username = check.finish_with(username)?;
    Ok(Registration { username, email })
}

pub fn login(body: &LoginBody) -> Result<Username, Invalid> {
    let mut check = Validator::new();
    let username = check.parse::<Username>("username", &body.username);
    check.finish_with(username)
}

pub fn view(account: &Account) -> UserView {
    UserView::named(UserId(account.user_id), &account.username)
}

pub fn reply(account: Account) -> AccountReply {
    AccountReply {
        user_id: account.user_id,
        username: account.username,
    }
}
