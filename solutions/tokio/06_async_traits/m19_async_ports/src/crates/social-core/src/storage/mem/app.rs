use crate::app::{App, AppConfig};
use crate::error::Result;
use crate::network::Network;
use crate::notify::Notifiers;
use crate::ports::{PostStore, UserStore};

use super::MemRepo;

impl App {
    pub fn new<U, P>(network: Network<U, P>) -> Result<App>
    where
        U: UserStore + Send + 'static,
        P: PostStore + Send + 'static,
    {
        App::start(network, Notifiers::new(), AppConfig::default())
    }

    pub fn with_notifiers<U, P>(network: Network<U, P>, notifiers: Notifiers) -> Result<App>
    where
        U: UserStore + Send + 'static,
        P: PostStore + Send + 'static,
    {
        App::start(network, notifiers, AppConfig::default())
    }

    pub fn start<U, P>(
        network: Network<U, P>,
        notifiers: Notifiers,
        config: AppConfig,
    ) -> Result<App>
    where
        U: UserStore + Send + 'static,
        P: PostStore + Send + 'static,
    {
        App::with_repos(MemRepo::repos(network), notifiers, config)
    }
}
