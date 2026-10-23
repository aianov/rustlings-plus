use std::sync::Arc;
use std::time::Duration;

use contracts::{ErrorBody, ErrorCode};
use tokio::net::TcpStream;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::mpsc::Receiver;
use tokio::time::{Instant, interval_at, sleep, sleep_until, timeout, timeout_at};
use websocket_engine::{Message, WsStream};

use super::work::Work;
use super::ws::{EXPIRED, GOING_AWAY, UNSUPPORTED};
use crate::bootstrap::Shared;
use crate::client::InboxGuard;
use crate::common::{Out, Outbox, TOO_SLOW, Visitor, unix_now};
use crate::controller::protocol;
use crate::routes::Ctx;

const GRACE: Duration = Duration::from_secs(2);

const MIN_PING: Duration = Duration::from_millis(10);

const NEVER: Duration = Duration::from_secs(365 * 24 * 60 * 60);

enum Exit {
    Stopping,
    Kicked,
    Silent,
    Unsupported,
    Gone,
    Stuck,
    Expired(Option<u64>),
}

impl Exit {
    fn can_answer(&self) -> bool {
        matches!(self, Exit::Stopping | Exit::Silent | Exit::Expired(..))
    }
}

enum Outgoing {
    Text(String),
    Ping,
}

async fn transmit(
    ws: &mut WsStream<TcpStream>,
    outgoing: Outgoing,
    outbox: &Outbox,
    shared: &Shared,
) -> Result<(), Exit> {
    let sending = async {
        match outgoing {
            Outgoing::Text(text) => ws.send(Message::Text(text)).await,
            Outgoing::Ping => ws.ping(b"").await,
        }
    };
    tokio::pin!(sending);
    let reason = tokio::select! {
        biased;
        sent = &mut sending => return sent.map_err(|_| Exit::Gone),
        () = outbox.kicked() => Exit::Kicked,
        () = shared.shutdown.cancelled() => Exit::Stopping,
    };
    match timeout(GRACE, sending).await {
        Ok(Ok(())) => Err(reason),
        Ok(Err(..)) => Err(Exit::Gone),
        Err(..) => Err(Exit::Stuck),
    }
}

async fn settle(
    ws: &mut WsStream<TcpStream>,
    queue: &mut Receiver<Out>,
    work: &mut Work,
    mut answering: bool,
) -> bool {
    let until = Instant::now() + GRACE;
    let mut ready = Vec::new();
    while let Ok(Out::Text(text)) = queue.try_recv() {
        ready.push(text);
    }
    loop {
        for text in ready.drain(..) {
            let sent = answering
                && timeout_at(until, ws.send(Message::Text(text)))
                    .await
                    .is_ok_and(|sent| sent.is_ok());
            answering &= sent;
        }
        if work.is_empty() {
            break;
        }
        match timeout_at(until, work.next()).await {
            Ok(Some(text)) => ready.push(text),
            Ok(None) | Err(..) => break,
        }
    }
    work.abort();
    answering
}

pub(super) async fn serve(
    mut ws: WsStream<TcpStream>,
    shared: Arc<Shared>,
    visitor: Visitor,
    permit: OwnedSemaphorePermit,
    inbox: Option<InboxGuard>,
) {
    let _permit = permit;
    let _inbox = inbox;
    let config = &shared.config;
    let (outbox, mut queue) = shared.registry.outbox(config.outbox);
    let registration = visitor
        .user()
        .map(|user| shared.registry.register(user, outbox.clone()));
    let ctx = Ctx::new(
        shared.upstreams.clone(),
        Arc::clone(&shared.registry),
        config.budget,
    )
    .parts(config.part_budget)
    .on(outbox.clone());
    let expires = match &visitor {
        Visitor::Member(principal) => Some(Instant::now() + principal.left(unix_now())),
        Visitor::Guest => None,
    };
    let deadline = expires.unwrap_or_else(|| Instant::now() + NEVER);
    let mut work = Work::new(config.max_in_flight);
    let every = config.ping_every.max(MIN_PING);
    let mut ticker = interval_at(Instant::now() + every, every);
    let exit = loop {
        if shared.shutdown.is_cancelled() {
            break Exit::Stopping;
        }
        let quiet = config.idle_timeout.saturating_sub(ws.idle_for());
        tokio::select! {
            () = shared.shutdown.cancelled() => break Exit::Stopping,
            item = queue.recv() => match item {
                Some(Out::Text(text)) if !outbox.is_kicked() => {
                    if let Err(exit) = transmit(&mut ws, Outgoing::Text(text), &outbox, &shared).await {
                        break exit;
                    }
                }
                Some(..) => break Exit::Kicked,
                None => break Exit::Gone,
            },
            Some(text) = work.next(), if !work.is_empty() => {
                outbox.send(text);
            }
            message = ws.recv() => match message {
                Ok(Some(Message::Text(text))) => {
                    if Instant::now() >= deadline {
                        break Exit::Expired(protocol::id_of(&text));
                    }
                    if let Some(reply) = work.start(&shared.router, &ctx, &visitor, &text) {
                        outbox.send(reply);
                    }
                }
                Ok(Some(Message::Binary(..))) => break Exit::Unsupported,
                Ok(None) | Err(..) => break Exit::Gone,
            },
            () = sleep(quiet) => break Exit::Silent,
            () = sleep_until(deadline), if expires.is_some() => break Exit::Expired(None),
            _ = ticker.tick() => {
                if let Err(exit) = transmit(&mut ws, Outgoing::Ping, &outbox, &shared).await {
                    break exit;
                }
            }
        }
    };
    drop(registration);
    if let Some(watching) = ctx.watching() {
        watching.stop();
    }
    let answering = settle(&mut ws, &mut queue, &mut work, exit.can_answer()).await;
    let (code, reason) = match exit {
        Exit::Stopping => (GOING_AWAY, "шлюз останавливается"),
        Exit::Silent => (GOING_AWAY, "клиент молчит дольше idle_timeout"),
        Exit::Kicked => (TOO_SLOW, "клиент не успевает читать"),
        Exit::Unsupported => (UNSUPPORTED, "конверты только текстом"),
        Exit::Expired(id) => {
            let notice = protocol::answer(id, None, Err(ErrorBody::public(ErrorCode::Expired)));
            if answering {
                let _ = timeout(GRACE, ws.send(Message::Text(notice))).await;
            }
            (EXPIRED, "срок токена вышел")
        }
        Exit::Gone | Exit::Stuck => return,
    };
    let _ = timeout(GRACE, ws.close(code, reason)).await;
}
