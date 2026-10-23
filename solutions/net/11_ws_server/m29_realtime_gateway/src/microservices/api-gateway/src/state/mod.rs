use std::sync::Arc;

use service_runtime::CancellationToken;
use social_core::App;
use tokio::sync::Semaphore;

use crate::auth::{Authenticator, Tokens};
use crate::config::Config;
use crate::media::Avatars;
use crate::registry::Registry;
use crate::routes::Routes;

pub(crate) struct Shared {
    pub(crate) app: App,
    pub(crate) registry: Arc<Registry>,
    pub(crate) tokens: Arc<Tokens>,
    pub(crate) auth: Arc<dyn Authenticator>,
    pub(crate) routes: Routes,
    pub(crate) avatars: Avatars,
    pub(crate) sessions: Arc<Semaphore>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) config: Config,
}
