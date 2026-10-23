use std::fmt;
use std::future::Future;
use std::pin::Pin;

use tokio::net::TcpStream;

pub struct Upgraded {
    pub stream: TcpStream,
    pub rest: Vec<u8>,
}

pub type UpgradeFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

pub struct UpgradeHook(pub Box<dyn FnOnce(Upgraded) -> UpgradeFuture + Send>);

impl UpgradeHook {
    pub fn new<F, Fut>(f: F) -> UpgradeHook
    where
        F: FnOnce(Upgraded) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        UpgradeHook(Box::new(move |upgraded| Box::pin(f(upgraded))))
    }
}

impl fmt::Debug for UpgradeHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UpgradeHook")
    }
}
