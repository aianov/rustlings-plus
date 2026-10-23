use std::sync::Arc;

use service_runtime::CancellationToken;
use social_core::App;
use tokio::sync::Semaphore;

use crate::auth::Tokens;
use crate::config::Config;
use crate::hub::Hub;
use crate::media::Avatars;

pub(crate) struct Shared {
    pub(crate) app: App,
    pub(crate) hub: Arc<Hub>,
    pub(crate) tokens: Tokens,
    pub(crate) avatars: Avatars,
    pub(crate) sessions: Arc<Semaphore>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) config: Config,
}
