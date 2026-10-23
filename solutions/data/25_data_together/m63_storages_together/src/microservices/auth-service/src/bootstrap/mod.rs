mod state;
mod stores;

use std::sync::Arc;
use std::time::Duration;

use admin::Admin;
use contracts::SystemClock;
use grpc_kit::{AccessLog, Readiness, Upstream, channel, unready_on_stop};
use jobs::{Checkpoints, Leader, Leases, Queue, QueueSpec, SagaLog, Worker, WorkerSpec, now_ms};
use nats_engine::{Bus, fresh_id};
use proto::social::v1::auth_server::AuthServer;
use proto::social::v1::users_server;
use service_runtime::{Background, CancellationToken, Criticality, Lifecycle, Restart};
use tls_edge::{mutual_server_config, terminate};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Channel;
use tonic_health::server::health_reporter;

use resp_client::Client;

use crate::client::{BusRevoker, JobDeletions, LogMailer, RedisActivity, SagaPosts, SagaProfiles};
use crate::common::{Config, Error, NAME, Result};
use crate::controller::{AuthController, app, jobs as chores};
use crate::service::{
    AuthParts, AuthService, DELETE_KIND, Deletion, DeletionQueue, KeyRing, Passwords, Revoker,
    SESSION_IDLE, THROTTLE_FORGET, Unheard,
};

pub use state::Shared;

type Served = AuthServer<AuthController>;

const SWEEP_RESTART: Restart = Restart {
    pause: Duration::from_secs(1),
    max_restarts: 3,
};

pub async fn connect(config: &Config) -> Result<Option<Bus>> {
    match &config.nats {
        Some(nats) => Bus::connect(nats.clone())
            .await
            .map(Some)
            .map_err(Error::Bus),
        None => Ok(None),
    }
}

pub async fn build(
    config: Config,
    bus: Option<Bus>,
    lifecycle: &Lifecycle,
    background: &mut Background,
) -> Result<Arc<Shared>> {
    let users =
        channel(&config.users, "user-service", config.tls.as_ref()).map_err(Error::Upstream)?;
    let upstream = Upstream {
        label: "user-service",
        service: users_server::SERVICE_NAME,
        channel: users.clone(),
    };
    Readiness::supervised(vec![upstream], background).register(lifecycle.health());
    let stores = stores::open(&config).await?;
    let (accounts, sessions, once) = (stores.accounts, stores.sessions, stores.once);
    let passwords = Passwords::new(config.cost, config.hash_parallel).map_err(Error::Password)?;
    let keys = match config.keys.clone() {
        Some(keys) => keys,
        None => {
            config.trail.note(
                "ключ подписи сгенерирован при старте: после перезапуска выданные токены не примутся, задай AUTH_SIGNING_KEYS",
            );
            KeyRing::random().ok_or(Error::Random)?
        }
    };
    let revoker: Arc<dyn Revoker> = match &bus {
        Some(bus) => Arc::new(BusRevoker::new(bus.clone(), config.topics.clone())),
        None => Arc::new(Unheard),
    };
    let service = AuthService::new(AuthParts {
        accounts,
        sessions,
        once,
        passwords: Arc::new(passwords),
        keys,
        clock: Arc::new(SystemClock),
        mailer: Arc::new(LogMailer::new(config.trail.clone())),
        lifetimes: config.lifetimes,
        moderators: config.moderators.clone(),
        public_url: config.public_url.clone(),
        revoker,
    });
    let service = Arc::new(match &config.redis {
        Some(redis) => service.with_activity(Arc::new(RedisActivity::new(
            Client::connect(redis.clone()),
            THROTTLE_FORGET,
            SESSION_IDLE,
        ))),
        None => service,
    });
    let deletions = match bus {
        Some(bus) => Some(chores_of(&config, bus, &service, &users, lifecycle, background).await?),
        None => {
            let sweeper = Arc::clone(&service);
            let every = config.sweep_every;
            background.spawn(
                "чистка сессий",
                Criticality::Optional,
                SWEEP_RESTART,
                move || {
                    let service = Arc::clone(&sweeper);
                    async move {
                        loop {
                            tokio::time::sleep(every).await;
                            service.purge().await;
                        }
                    }
                },
            );
            None
        }
    };
    Ok(Arc::new(Shared {
        service,
        users,
        deletions,
        config,
    }))
}

async fn chores_of(
    config: &Config,
    bus: Bus,
    service: &Arc<AuthService>,
    users: &Channel,
    lifecycle: &Lifecycle,
    background: &mut Background,
) -> Result<Arc<dyn DeletionQueue>> {
    let topics = &config.topics;
    let spec = QueueSpec {
        stream: topics.jobs_stream(),
        prefix: topics.jobs(),
        dead_stream: topics.jobs_dead_stream(),
        dead_prefix: topics.jobs_dead(),
    };
    let queue = Queue::open(bus.clone(), spec).await.map_err(Error::Bus)?;
    let posts =
        channel(&config.posts, "post-service", config.tls.as_ref()).map_err(Error::Upstream)?;
    let deletion = Arc::new(Deletion {
        profiles: Arc::new(SagaProfiles::new(users.clone())),
        posts: Arc::new(SagaPosts::new(posts)),
        log: SagaLog::open(&bus, &topics.sagas())
            .await
            .map_err(Error::Bus)?,
        checkpoints: Checkpoints::open(&bus, &topics.checkpoints())
            .await
            .map_err(Error::Bus)?,
    });
    let every = config.sweep_every;
    let leases = format!("{}_SWEEP", topics.leases());
    let leases = Leases::open(&bus, &leases, every * 2)
        .await
        .map_err(Error::Bus)?;
    let leader = Leader::new("sweep", &fresh_id(NAME), every, leases, now_ms());
    tokio::spawn(chores::sweep(
        Arc::clone(service),
        leader,
        lifecycle.closing(),
    ));
    let handler = chores::deletions(Arc::clone(service), deletion);
    let worker_spec = WorkerSpec {
        kind: Some(DELETE_KIND.to_string()),
        ..WorkerSpec::new("auth-delete")
    };
    let (closing, worker_queue) = (lifecycle.closing(), queue.clone());
    background.spawn(
        "удаление аккаунтов",
        Criticality::Optional,
        SWEEP_RESTART,
        move || {
            let (queue, spec, handler, closing) = (
                worker_queue.clone(),
                worker_spec.clone(),
                Arc::clone(&handler),
                closing.clone(),
            );
            async move {
                match Worker::start(&queue, spec, handler).await {
                    Ok(worker) => {
                        closing.cancelled().await;
                        worker.stop().await;
                    }
                    Err(error) => tracing::warn!("{NAME}: воркер удаления не запустился: {error}"),
                }
            }
        },
    );
    Ok(Arc::new(JobDeletions::new(queue)))
}

pub async fn run(
    http: TcpListener,
    grpc: TcpListener,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
    launch(http, grpc, None, config, shutdown).await
}

pub async fn serve(
    http: TcpListener,
    grpc: TcpListener,
    admin: TcpListener,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
    launch(http, grpc, Some(admin), config, shutdown).await
}

async fn launch(
    http: TcpListener,
    grpc: TcpListener,
    admin: Option<TcpListener>,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
    let lifecycle = config.lifecycle(&shutdown);
    let mut background = Background::new(config.trail.clone());
    let (tls, log, wait) = (config.tls.clone(), config.log.clone(), config.timing.wait);
    let bus = connect(&config).await?;
    let shared = build(config, bus.clone(), &lifecycle, &mut background).await?;
    let auth = AuthServer::new(AuthController::new(Arc::clone(&shared.service)));
    let web = http_engine::Server::with_handler(app(Arc::clone(&shared)))
        .config(shared.config.http.clone());
    drop(shared);
    let admin = Admin::start(admin, &lifecycle);
    let (reporter, health) = health_reporter();
    reporter.set_serving::<Served>().await;
    tokio::spawn(unready_on_stop::<Served>(
        reporter,
        lifecycle.health().clone(),
    ));
    let closing = lifecycle.closing();
    let (engine, edge) = match &tls {
        Some(tls) => {
            let inner = TcpListener::bind("127.0.0.1:0").await.map_err(Error::Io)?;
            let inner_addr = inner.local_addr().map_err(Error::Io)?;
            let config =
                mutual_server_config(&tls.cert, tls.key.expose(), &tls.ca).map_err(Error::Certs)?;
            let edge = terminate(http, inner_addr, config, closing.clone());
            (inner, Some(edge))
        }
        None => (http, None),
    };
    let builder = match &tls {
        Some(tls) => tonic::transport::Server::builder()
            .tls_config(tls.server())
            .map_err(Error::Tls)?,
        None => tonic::transport::Server::builder(),
    };
    let web_closing = closing.clone();
    let web = async move {
        web.serve(engine, async move { web_closing.cancelled().await })
            .await
            .map_err(Error::Io)
    };
    let edge = async move {
        match edge {
            Some(edge) => edge.await.map_err(Error::Io),
            None => Ok(()),
        }
    };
    let rpc = async move {
        builder
            .layer(AccessLog::new(NAME, log))
            .add_service(health)
            .add_service(auth)
            .serve_with_incoming_shutdown(TcpListenerStream::new(grpc), async move {
                closing.cancelled().await;
            })
            .await
            .map_err(Error::Serve)
    };
    let serving = async move { tokio::try_join!(web, edge, rpc).map(|_| ()) };
    let halted = lifecycle.run(wait, serving, background).await;
    if let Some(bus) = bus {
        bus.close().await.map_err(Error::Bus)?;
    }
    admin.stop().await.map_err(Error::Io)?;
    halted.map_err(Error::from)
}
