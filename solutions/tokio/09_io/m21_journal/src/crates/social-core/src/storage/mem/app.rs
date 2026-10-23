use std::path::Path;

use crate::app::{App, AppConfig};
use crate::error::Result;
use crate::journal::{Journal, load};
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

    pub async fn restore<U, P>(
        mut network: Network<U, P>,
        notifiers: Notifiers,
        config: AppConfig,
        path: impl AsRef<Path>,
    ) -> Result<App>
    where
        U: UserStore + Send + 'static,
        P: PostStore + Send + 'static,
    {
        let path = path.as_ref();
        let entries = load(path, config.journal_max_line).await?;
        network.restore(&entries)?;
        let journal = Journal::open(path).await?;
        App::journaled(MemRepo::repos(network), notifiers, config, journal)
    }
}
