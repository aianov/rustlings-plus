use std::future::Future;
use std::time::Duration;

use service_runtime::CancellationToken;
use tokio::sync::watch;
use tokio::time::{sleep, timeout};
use tonic::server::NamedService;
use tonic::transport::Channel;
use tonic_health::ServingStatus;
use tonic_health::pb::HealthCheckRequest;
use tonic_health::pb::health_check_response::ServingStatus as Seen;
use tonic_health::pb::health_client::HealthClient;
use tonic_health::server::HealthReporter;

const FIRST_RETRY: Duration = Duration::from_millis(50);

const LAST_RETRY: Duration = Duration::from_secs(1);

pub const HARD_STOP: Duration = Duration::from_secs(2);

pub async fn drain<S: NamedService>(
    reporter: HealthReporter,
    shutdown: CancellationToken,
    grace: Duration,
) {
    shutdown.cancelled().await;
    reporter
        .set_service_status(S::NAME, ServingStatus::NotServing)
        .await;
    sleep(grace).await;
}

pub async fn bounded<F, E>(
    serving: F,
    shutdown: CancellationToken,
    limit: Duration,
) -> Result<(), E>
where
    F: Future<Output = Result<(), E>>,
{
    tokio::pin!(serving);
    tokio::select! {
        served = &mut serving => served,
        () = async {
            shutdown.cancelled().await;
            sleep(limit).await;
        } => Ok(()),
    }
}

#[derive(Debug, Clone)]
pub struct Upstream {
    pub label: &'static str,
    pub service: &'static str,
    pub channel: Channel,
}

#[derive(Debug, Clone)]
pub struct Readiness {
    state: watch::Receiver<Vec<(&'static str, bool)>>,
}

async fn follow(
    upstream: Upstream,
    index: usize,
    state: watch::Sender<Vec<(&'static str, bool)>>,
    stop: CancellationToken,
) {
    let mark = |ready: bool| {
        state.send_modify(|all| all[index].1 = ready);
    };
    let mut pause = FIRST_RETRY;
    while !stop.is_cancelled() {
        let mut client = HealthClient::new(upstream.channel.clone());
        let ask = HealthCheckRequest {
            service: upstream.service.to_string(),
        };
        if let Ok(stream) = client.watch(ask).await {
            let mut stream = stream.into_inner();
            loop {
                let next = tokio::select! {
                    next = stream.message() => next,
                    () = stop.cancelled() => return,
                };
                match next {
                    Ok(Some(reply)) if reply.status() == Seen::Serving => {
                        pause = FIRST_RETRY;
                        mark(true);
                    }
                    Ok(Some(..) | None) | Err(..) => break,
                }
            }
        }
        mark(false);
        tokio::select! {
            () = sleep(pause) => {}
            () = stop.cancelled() => return,
        }
        pause = (pause * 2).min(LAST_RETRY);
    }
}

impl Readiness {
    pub fn watch(upstreams: Vec<Upstream>, stop: &CancellationToken) -> Readiness {
        let initial = upstreams.iter().map(|up| (up.label, false)).collect();
        let (state, receiver) = watch::channel(initial);
        for (index, upstream) in upstreams.into_iter().enumerate() {
            tokio::spawn(follow(upstream, index, state.clone(), stop.clone()));
        }
        Readiness { state: receiver }
    }

    pub fn missing(&self) -> Vec<&'static str> {
        self.state
            .borrow()
            .iter()
            .filter(|(_, ready)| !ready)
            .map(|(label, _)| *label)
            .collect()
    }

    pub fn is_ready(&self) -> bool {
        self.missing().is_empty()
    }

    pub async fn wait(&self, limit: Duration) -> Result<(), Vec<&'static str>> {
        let mut state = self.state.clone();
        let all = state.wait_for(|all| all.iter().all(|(_, ready)| *ready));
        match timeout(limit, all).await {
            Ok(Ok(..)) => Ok(()),
            _ => Err(self.missing()),
        }
    }
}
