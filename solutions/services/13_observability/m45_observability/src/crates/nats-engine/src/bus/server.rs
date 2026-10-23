use std::future::Future;

use async_nats::{Client, Subscriber};
use futures::StreamExt;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use service_runtime::{TraceContext, with_trace};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::Instrument;

use super::round_trip;
use super::subscription::{Envelope, open};
use crate::meta::Meta;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reply<A> {
    Ok(A),
    Err(String),
}

pub struct Server {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl Server {
    pub(crate) fn start<Q, A, H, F>(client: Client, mut requests: Subscriber, handler: H) -> Server
    where
        Q: DeserializeOwned + Send + 'static,
        A: Serialize + Send + 'static,
        H: Fn(Envelope<Q>) -> F + Send + Sync + 'static,
        F: Future<Output = Result<A, String>> + Send + 'static,
    {
        let (stop, mut stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            loop {
                let message = tokio::select! {
                    message = requests.next() => message,
                    _ = &mut stopped => None,
                };
                let Some(message) = message else { break };
                let Some(reply_to) = message.reply.clone() else {
                    continue;
                };
                let reply: Reply<A> = match open::<Q>(&message) {
                    Ok(envelope) => match serve_one(&handler, envelope).await {
                        Ok(answer) => Reply::Ok(answer),
                        Err(text) => Reply::Err(text),
                    },
                    Err(error) => Reply::Err(error.to_string()),
                };
                let Ok(payload) = serde_json::to_vec(&reply) else {
                    continue;
                };
                let sent = match Meta::from_headers(message.headers.as_ref()) {
                    Some(meta) => {
                        client
                            .publish_with_headers(reply_to, meta.to_headers(), payload.into())
                            .await
                    }
                    None => client.publish(reply_to, payload.into()).await,
                };
                let _ = sent;
            }
            let _ = requests.unsubscribe().await;
            let _ = round_trip(&client).await;
        });
        Server { stop, task }
    }

    pub async fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.task.await;
    }
}

async fn serve_one<Q, A, H, F>(handler: &H, envelope: Envelope<Q>) -> Result<A, String>
where
    H: Fn(Envelope<Q>) -> F,
    F: Future<Output = Result<A, String>>,
{
    let trace = envelope
        .meta
        .as_ref()
        .map_or_else(TraceContext::fresh, Meta::trace);
    let span = tracing::info_span!(
        "request",
        subject = %envelope.subject.as_str(),
        trace_id = %trace.trace_id,
    );
    with_trace(trace, handler(envelope)).instrument(span).await
}
