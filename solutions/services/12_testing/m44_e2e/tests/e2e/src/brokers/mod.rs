use std::time::Duration;

use contracts::Topics;
use nats_engine::{BusConfig, DEFAULT_NATS_URL};
use test_support::Eventually;

pub const NATS_ENV: &str = "RUSTLINGS_NATS_URL";

pub fn nats_url() -> String {
    std::env::var(NATS_ENV)
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string())
}

pub fn nats(name: &str) -> BusConfig {
    BusConfig::new(nats_url()).named(name)
}

fn names(topics: &Topics) -> Vec<String> {
    let kv = |bucket: String| format!("KV_{bucket}");
    vec![
        topics.stream(),
        topics.chat_stream(),
        topics.jobs_stream(),
        topics.jobs_dead_stream(),
        kv(topics.presence()),
        kv(topics.checkpoints()),
        kv(topics.sagas()),
        kv(format!("{}_SWEEP", topics.leases())),
        kv(format!("{}_DIGEST", topics.leases())),
        kv(format!("{}_ORPHANS", topics.leases())),
    ]
}

fn gone(error: &async_nats::jetstream::context::GetStreamError) -> bool {
    match error.kind() {
        async_nats::jetstream::context::GetStreamErrorKind::JetStream(inner) => {
            inner.error_code() == async_nats::jetstream::ErrorCode::STREAM_NOT_FOUND
        }
        _ => false,
    }
}

pub async fn drop_streams(topics: &Topics) {
    for name in names(topics) {
        Eventually::within(Duration::from_secs(5))
            .until(&format!("stream {name} удаляется"), || {
                let name = name.clone();
                async move {
                    let client = async_nats::connect(nats_url())
                        .await
                        .map_err(|error| error.to_string())?;
                    let js = async_nats::jetstream::new(client);
                    let _ = js.delete_stream(&name).await;
                    match js.get_stream(&name).await {
                        Err(error) if gone(&error) => Ok(()),
                        Err(error) => Err(error.to_string()),
                        Ok(..) => Err(String::from("ещё есть")),
                    }
                }
            })
            .await
            .unwrap_or_else(|gave_up| panic!("{gave_up}: тест оставит мусор в брокере курса"));
    }
}
