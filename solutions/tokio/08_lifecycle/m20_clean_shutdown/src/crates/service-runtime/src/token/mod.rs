use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::watch;

#[derive(Clone)]
pub struct CancellationToken {
    own: Arc<watch::Sender<bool>>,
    parent: Option<Box<CancellationToken>>,
}

impl Default for CancellationToken {
    fn default() -> CancellationToken {
        CancellationToken::new()
    }
}

impl CancellationToken {
    pub fn new() -> CancellationToken {
        CancellationToken {
            own: Arc::new(watch::channel(false).0),
            parent: None,
        }
    }

    pub fn child(&self) -> CancellationToken {
        CancellationToken {
            own: Arc::new(watch::channel(false).0),
            parent: Some(Box::new(self.clone())),
        }
    }

    pub fn cancel(&self) {
        self.own.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.own.borrow()
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.is_cancelled())
    }

    pub fn cancelled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let mut own = self.own.subscribe();
            match &self.parent {
                Some(parent) => {
                    tokio::select! {
                        _ = own.wait_for(|cancelled| *cancelled) => {}
                        () = parent.cancelled() => {}
                    }
                }
                None => {
                    let _ = own.wait_for(|cancelled| *cancelled).await;
                }
            }
        })
    }
}
