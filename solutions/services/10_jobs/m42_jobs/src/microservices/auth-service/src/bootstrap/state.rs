use std::sync::Arc;

use tonic::transport::Channel;

use crate::common::Config;
use crate::service::{AuthService, DeletionQueue};

pub struct Shared {
    pub service: Arc<AuthService>,
    pub users: Channel,
    pub deletions: Option<Arc<dyn DeletionQueue>>,
    pub config: Config,
}
