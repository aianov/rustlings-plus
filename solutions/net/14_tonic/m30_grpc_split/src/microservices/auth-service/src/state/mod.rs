use std::sync::Arc;

use proto::social::v1::users_client::UsersClient;
use tonic::transport::Channel;

use crate::accounts::Accounts;
use crate::config::Config;
use crate::tokens::Tokens;

pub(crate) struct Shared {
    pub(crate) accounts: Arc<Accounts>,
    pub(crate) tokens: Arc<Tokens>,
    pub(crate) users: UsersClient<Channel>,
    pub(crate) config: Config,
}
