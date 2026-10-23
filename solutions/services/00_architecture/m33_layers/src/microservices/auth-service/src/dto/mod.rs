use contracts::{UserId, UserView};
use proto::social::v1::Account as AccountReply;

use crate::common::Account;

pub fn view(account: &Account) -> UserView {
    UserView {
        id: UserId(account.user_id),
        username: account.username.clone(),
    }
}

pub fn reply(account: Account) -> AccountReply {
    AccountReply {
        user_id: account.user_id,
        username: account.username,
    }
}
