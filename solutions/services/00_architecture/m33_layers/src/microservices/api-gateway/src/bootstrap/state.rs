use std::sync::Arc;

use grpc_kit::Readiness;
use service_runtime::CancellationToken;
use tokio::sync::Semaphore;

use crate::client::Inboxes;
use crate::client::RemoteAuth;
use crate::client::Upstreams;
use crate::common::Config;
use crate::common::Registry;
use crate::routes::Routes;

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
