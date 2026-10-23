use std::sync::Arc;

use grpc_kit::Readiness;
use service_runtime::CancellationToken;
use tokio::sync::Semaphore;

use crate::auth::RemoteAuth;
use crate::config::Config;
use crate::inbox::Inboxes;
use crate::registry::Registry;
use crate::routes::Routes;
use crate::upstream::Upstreams;

pub(crate) struct Shared {
    pub(crate) upstreams: Upstreams,
    pub(crate) auth: RemoteAuth,
    pub(crate) registry: Arc<Registry>,
    pub(crate) inboxes: Arc<Inboxes>,
    pub(crate) readiness: Readiness,
    pub(crate) routes: Routes,
    pub(crate) sessions: Arc<Semaphore>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) config: Config,
}
