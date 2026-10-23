use std::sync::Arc;

use grpc_kit::Readiness;
use service_runtime::CancellationToken;
use tokio::sync::Semaphore;

use crate::client::AuthHttp;
use crate::client::Notify;
use crate::client::RemoteAuth;
use crate::client::Upstreams;
use crate::common::Config;
use crate::common::Links;
use crate::common::Registry;
use crate::controller::presence::Realtime;
use crate::routes::Router;

pub(crate) struct Shared {
    pub(crate) upstreams: Upstreams,
    pub(crate) auth: RemoteAuth,
    pub(crate) auth_http: AuthHttp,
    pub(crate) registry: Arc<Registry>,
    pub(crate) realtime: Option<Arc<Realtime>>,
    pub(crate) notify: Option<Notify>,
    pub(crate) readiness: Readiness,
    pub(crate) router: Router,
    pub(crate) sessions: Arc<Semaphore>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) links: Links,
    pub(crate) config: Config,
}
