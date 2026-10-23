use std::sync::Arc;

use framing::{Framed, LinesCodec, LinesError};
use service_runtime::{CancellationToken, TaskTracker};
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tokio::time::timeout;

use crate::protocol::{self, State};
use crate::server::Shared;

pub(crate) async fn serve(
    stream: TcpStream,
    shared: Arc<Shared>,
    shutdown: CancellationToken,
    tasks: TaskTracker,
    permit: OwnedSemaphorePermit,
) {
    let _permit = permit;
    let (read, write) = stream.into_split();
    let codec = LinesCodec::new(shared.config.max_line);
    let mut frames = Framed::new(read, codec);
    let (outbox, inbox) = mpsc::channel(shared.config.outbox.max(1));
    let console = shared.app.console();
    let writer =
        tasks.spawn(console.traced("gateway-writer", deliver(Framed::new(write, codec), inbox)));
    let mut state = State::new(outbox.clone());
    loop {
        let next = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            next = timeout(shared.config.idle_timeout, frames.recv()) => next,
        };
        let line = match next {
            Ok(Ok(Some(line))) => line,
            Ok(Err(error @ LinesError::BadUtf8)) => {
                if outbox
                    .send(protocol::failure("bad_utf8", &error))
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
            Ok(Err(error @ LinesError::TooLong { .. })) => {
                let _ = outbox.send(protocol::failure("too_long", &error)).await;
                break;
            }
            Ok(Err(LinesError::Io(..)) | Ok(None)) | Err(..) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let reply = protocol::handle(&shared, &mut state, &line).await;
        if outbox.send(reply).await.is_err() {
            break;
        }
    }
    drop(state);
    drop(outbox);
    let _ = writer.await;
}

async fn deliver(
    mut frames: Framed<OwnedWriteHalf, LinesCodec>,
    mut inbox: mpsc::Receiver<String>,
) {
    while let Some(line) = inbox.recv().await {
        if frames.send(line).await.is_err() {
            return;
        }
    }
    let _ = frames.close().await;
}
