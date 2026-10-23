use std::sync::Arc;
use std::time::Duration;

use contracts::UserView;
use tokio::net::TcpStream;
use tokio::sync::OwnedSemaphorePermit;
use tokio::task::JoinSet;
use tokio::time::{Instant, interval_at, sleep, timeout};
use websocket_engine::{Message, WsStream};

use super::{GOING_AWAY, UNSUPPORTED};
use crate::inbox::InboxGuard;
use crate::protocol;
use crate::registry::{Out, Outbox, TOO_SLOW};
use crate::routes::Ctx;
use crate::state::Shared;

const GRACE: Duration = Duration::from_secs(2);

const MIN_PING: Duration = Duration::from_millis(10);

enum Exit {
    Stopping,
    Kicked,
    Silent,
    Unsupported,
    Gone,
    Stuck,
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

fn start(
    tasks: &mut JoinSet<()>,
    shared: &Shared,
    ctx: &Ctx,
    user: &UserView,
    outbox: &Outbox,
    text: &str,
) {
    if text.trim().is_empty() {
        return;
    }
    let envelope = match protocol::request(text) {
        Ok(envelope) => envelope,
        Err(reply) => {
            outbox.send(reply);
            return;
        }
    };
    let id = Some(envelope.id);
    let work = shared
        .routes
        .dispatch(ctx.for_request(user.id), user, envelope);
    let outbox = outbox.clone();
    tasks.spawn(async move {
        let answer = match tokio::spawn(work).await {
            Ok(answer) => answer,
            Err(..) => Err(protocol::internal("обработчик метода упал")),
        };
        outbox.send(protocol::answer(id, answer));
    });
}

pub(super) async fn serve(
    mut ws: WsStream<TcpStream>,
    shared: Arc<Shared>,
    user: UserView,
    permit: OwnedSemaphorePermit,
    inbox: InboxGuard,
) {
    let _permit = permit;
    let _inbox = inbox;
    let config = &shared.config;
    let (outbox, mut queue) = shared.registry.outbox(config.outbox);
    let registration = shared.registry.register(user.id, outbox.clone());
    let ctx = Ctx::new(
        shared.upstreams.clone(),
        Arc::clone(&shared.registry),
        config.budget,
    )
    .on(outbox.clone());
    let mut tasks = JoinSet::new();
    let every = config.ping_every.max(MIN_PING);
    let mut ticker = interval_at(Instant::now() + every, every);
    let exit = loop {
        if shared.shutdown.is_cancelled() {
            break Exit::Stopping;
        }
        let reading = tasks.len() < config.max_in_flight.max(1);
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
            Some(..) = tasks.join_next(), if !tasks.is_empty() => {}
            message = ws.recv(), if reading => match message {
                Ok(Some(Message::Text(text))) => {
                    start(&mut tasks, &shared, &ctx, &user, &outbox, &text);
                }
                Ok(Some(Message::Binary(..))) => break Exit::Unsupported,
                Ok(None) | Err(..) => break Exit::Gone,
            },
            () = sleep(quiet), if reading => break Exit::Silent,
            _ = ticker.tick() => {
                if let Err(exit) = transmit(&mut ws, Outgoing::Ping, &outbox, &shared).await {
                    break exit;
                }
            }
        }
    };
    drop(registration);
    tasks.abort_all();
    if let Some(watching) = ctx.watching() {
        watching.stop();
    }
    let (code, reason) = match exit {
        Exit::Stopping => (GOING_AWAY, "шлюз останавливается"),
        Exit::Silent => (GOING_AWAY, "клиент молчит дольше idle_timeout"),
        Exit::Kicked => (TOO_SLOW, "клиент не успевает читать"),
        Exit::Unsupported => (UNSUPPORTED, "конверты только текстом"),
        Exit::Gone | Exit::Stuck => return,
    };
    let _ = timeout(GRACE, ws.close(code, reason)).await;
}
