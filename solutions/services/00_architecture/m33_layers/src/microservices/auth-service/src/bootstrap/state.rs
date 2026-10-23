use std::sync::Arc;

use tonic::transport::Channel;

use crate::common::Config;
use crate::service::AuthService;

pub struct Shared {
    pub service: Arc<AuthService>,
    pub users: Channel,
    pub config: Config,
}
