use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::sync::watch;
use websocket_engine::WsStream;

use crate::hub::Hub;
use crate::router::Router;
use crate::{Config, Stats};

pub struct Shared {
    pub hub: Hub,
    pub router: Router,
    pub config: Config,
    pub stats: Arc<Stats>,
}

use tokio::task::JoinSet;
use tokio::time::{Instant, interval_at, timeout};
use websocket_engine::Message;

use crate::envelope::{Failure, parse_request, render_err, render_ok};
use crate::hub::{Out, Outbox};
use crate::router::Ctx;

enum Exit {
    Stopping,
    Kicked,
    Silent,
    Gone,
}

pub async fn serve_connection(mut ws: WsStream<TcpStream>, user: String, shared: Arc<Shared>, mut stop: watch::Receiver<bool>) {
    let config = &shared.config;
    let (outbox, mut queue) = Outbox::channel(config.outbox, shared.stats.clone());
    let guard = shared.hub.connect(&user, outbox.clone());
    let ctx = Ctx { user, conn: guard.id(), hub: shared.hub.clone(), outbox: outbox.clone() };
    let mut tasks: JoinSet<()> = JoinSet::new();
    let mut ticker = interval_at(Instant::now() + config.ping_every, config.ping_every);
    let exit = loop {
        if *stop.borrow() {
            break Exit::Stopping;
        }
        let reading = tasks.len() < config.max_in_flight;
        tokio::select! {
            _ = stop.changed() => break Exit::Stopping,
            item = queue.recv() => match item {
                Some(Out::Text(text)) => {
                    tokio::select! {
                        sent = ws.send(Message::Text(text)) => {
                            if sent.is_err() {
                                break Exit::Gone;
                            }
                        }
                        _ = outbox.kicked() => break Exit::Kicked,
                    }
                }
                Some(Out::Close(_)) => break Exit::Kicked,
                None => break Exit::Gone,
            },
            message = ws.recv(), if reading => match message {
                Ok(Some(Message::Text(text))) => {
                    tasks.spawn(handle(text, ctx.clone(), shared.clone()));
                }
                Ok(Some(Message::Binary(_))) => {
                    let failure = Failure { id: None, code: "bad_request", message: "бинарные сообщения не поддерживаются".to_string() };
                    outbox.reply(render_err(&failure));
                }
                Ok(None) | Err(_) => break Exit::Gone,
            },
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
            _ = ticker.tick() => {
                if reading && ws.idle_for() > config.idle_limit {
                    break Exit::Silent;
                }
                if ws.ping(b"").await.is_err() {
                    break Exit::Gone;
                }
            }
        }
    };
    let patience = Duration::from_secs(1);
    match exit {
        Exit::Stopping => {
            let _ = timeout(patience * 2, ws.close(1001, "сервер перезапускается")).await;
        }
        Exit::Kicked => {
            let _ = timeout(patience, ws.close(1008, "клиент не успевает читать")).await;
        }
        Exit::Silent => {
            shared.stats.note_timed_out();
            let _ = timeout(patience, ws.close(1001, "нет ответа на ping")).await;
        }
        Exit::Gone => {}
    }
    drop(tasks);
    drop(guard);
}

async fn handle(text: String, ctx: Ctx, shared: Arc<Shared>) {
    let reply = match parse_request(&text) {
        Err(failure) => render_err(&failure),
        Ok(request) => {
            let id = request.id;
            let work = tokio::spawn(shared.router.dispatch(ctx.clone(), &request.service, &request.method, request.data));
            match work.await {
                Ok(Ok(data)) => render_ok(id, data),
                Ok(Err(error)) => render_err(&Failure { id: Some(id), code: error.code, message: error.message }),
                Err(_) => render_err(&Failure { id: Some(id), code: "internal", message: "обработчик упал".to_string() }),
            }
        }
    };
    ctx.outbox.reply(reply);
}
