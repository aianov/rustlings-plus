use std::future::Future;
use std::time::Duration;

use service_runtime::{Background, CancellationToken, Criticality, Health, Restart};
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

pub const WATCH_RESTART: Restart = Restart {
    pause: Duration::from_millis(200),
    max_restarts: 5,
};

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

type State = Vec<(&'static str, Result<(), String>)>;

#[derive(Debug, Clone)]
pub struct Readiness {
    state: watch::Receiver<State>,
}

pub const NOT_YET: &str = "ещё не отвечал";

async fn follow(
    upstream: Upstream,
    index: usize,
    state: watch::Sender<State>,
    stop: CancellationToken,
) {
    let mark = |seen: Result<(), String>| {
        state.send_modify(|all| all[index].1 = seen);
    };
    let mut pause = FIRST_RETRY;
    while !stop.is_cancelled() {
        let mut client = HealthClient::new(upstream.channel.clone());
        let ask = HealthCheckRequest {
            service: upstream.service.to_string(),
        };
        let reason = match client.watch(ask).await {
            Ok(stream) => {
                let mut stream = stream.into_inner();
                loop {
                    let next = tokio::select! {
                        next = stream.message() => next,
                        () = stop.cancelled() => return,
                    };
                    match next {
                        Ok(Some(reply)) if reply.status() == Seen::Serving => {
                            pause = FIRST_RETRY;
                            mark(Ok(()));
                        }
                        Ok(Some(reply)) => break format!("{:?}", reply.status()),
                        Ok(None) => break "поток здоровья закрылся".to_string(),
                        Err(status) => break format!("{:?}: {}", status.code(), status.message()),
                    }
                }
            }
            Err(status) => format!("нет связи: {}", status.message()),
        };
        mark(Err(reason));
        tokio::select! {
            () = sleep(pause) => {}
            () = stop.cancelled() => return,
        }
        pause = (pause * 2).min(LAST_RETRY);
    }
}

fn initial(upstreams: &[Upstream]) -> State {
    upstreams
        .iter()
        .map(|up| (up.label, Err(NOT_YET.to_string())))
        .collect()
}

impl Readiness {
    pub fn watch(upstreams: Vec<Upstream>, stop: &CancellationToken) -> Readiness {
        let (state, receiver) = watch::channel(initial(&upstreams));
        for (index, upstream) in upstreams.into_iter().enumerate() {
            tokio::spawn(follow(upstream, index, state.clone(), stop.clone()));
        }
        Readiness { state: receiver }
    }

    pub fn supervised(upstreams: Vec<Upstream>, background: &mut Background) -> Readiness {
        let (state, receiver) = watch::channel(initial(&upstreams));
        for (index, upstream) in upstreams.into_iter().enumerate() {
            let state = state.clone();
            let name = format!("здоровье {}", upstream.label);
            let make = move || {
                follow(
                    upstream.clone(),
                    index,
                    state.clone(),
                    CancellationToken::new(),
                )
            };
            background.spawn(name, Criticality::Critical, WATCH_RESTART, make);
        }
        Readiness { state: receiver }
    }

    pub fn missing(&self) -> Vec<&'static str> {
        self.state
            .borrow()
            .iter()
            .filter(|(_, seen)| seen.is_err())
            .map(|(label, _)| *label)
            .collect()
    }

    pub fn status(&self, label: &str) -> Result<(), String> {
        self.state
            .borrow()
            .iter()
            .find(|(known, _)| *known == label)
            .map_or_else(
                || Err(format!("{label} не отслеживается")),
                |(_, seen)| seen.clone(),
            )
    }

    pub fn register(&self, health: &Health) {
        let labels: Vec<&'static str> = self
            .state
            .borrow()
            .iter()
            .map(|(label, _)| *label)
            .collect();
        for label in labels {
            let readiness = self.clone();
            health.register(label, move || {
                let seen = readiness.status(label);
                async move { seen }
            });
        }
    }

    pub fn is_ready(&self) -> bool {
        self.missing().is_empty()
    }

    pub async fn wait(&self, limit: Duration) -> Result<(), Vec<&'static str>> {
        let mut state = self.state.clone();
        let all = state.wait_for(|all| all.iter().all(|(_, seen)| seen.is_ok()));
        match timeout(limit, all).await {
            Ok(Ok(..)) => Ok(()),
            _ => Err(self.missing()),
        }
    }
}

pub async fn unready_on_stop<S: NamedService>(reporter: HealthReporter, health: Health) {
    health.stopped().await;
    reporter
        .set_service_status(S::NAME, ServingStatus::NotServing)
        .await;
}
