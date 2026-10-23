use std::future::Future;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::token::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Restart {
    pub pause: Duration,
    pub max_restarts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Finished { restarts: u32 },
    GaveUp { attempts: u32 },
    Cancelled { restarts: u32 },
}

struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub async fn supervise<F, Fut>(make: F, token: CancellationToken, policy: Restart) -> Outcome
where
    F: Fn() -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let mut restarts = 0;
    loop {
        let mut run = AbortOnDrop(tokio::spawn(make()));
        tokio::select! {
            finished = &mut run.0 => {
                if finished.is_ok() {
                    return Outcome::Finished { restarts };
                }
                if restarts >= policy.max_restarts {
                    return Outcome::GaveUp { attempts: restarts + 1 };
                }
            }
            () = token.cancelled() => {
                run.0.abort();
                let _ = (&mut run.0).await;
                return Outcome::Cancelled { restarts };
            }
        }
        tokio::select! {
            () = sleep(policy.pause) => {}
            () = token.cancelled() => return Outcome::Cancelled { restarts },
        }
        restarts += 1;
    }
}
