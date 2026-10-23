use crate::{Bus, Headers, Ids, Message};
use obs::TRACEPARENT;
use obs::metrics::Registry;
use obs::red::Red;
use obs::trace::enter;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{Instrument, info};

#[derive(Debug, PartialEq)]
pub enum PostError {
    Empty,
    Broken,
}

impl PostError {
    pub fn status(&self) -> u16 {
        match self {
            PostError::Empty => 422,
            PostError::Broken => 500,
        }
    }
}

pub struct PostService {
    registry: Arc<Registry>,
    red: Red,
    bus: Bus,
    ids: Ids,
    next_post: AtomicU64,
}

impl PostService {
    pub fn new(bus: Bus, ids: Ids) -> PostService {
        let registry = Arc::new(Registry::new());
        PostService { red: Red::new(registry.clone()), registry, bus, ids, next_post: AtomicU64::new(1) }
    }

    pub fn metrics(&self) -> String {
        self.registry.render()
    }

    pub async fn create(&self, metadata: &Headers, body: &str) -> Result<u64, PostError> {
        let entered = enter(metadata.get(TRACEPARENT).map(String::as_str), &mut || (self.ids)());
        let span = obs::hop_span("posts", &entered.ctx, entered.parent);
        let work = async {
            let result = self.save(body).await;
            if let Ok(post_id) = &result {
                let mut headers = Headers::new();
                headers.insert(TRACEPARENT.to_string(), entered.ctx.header());
                self.bus.publish(Message { subject: "post.created".to_string(), headers, payload: format!("{{\"post_id\":{post_id}}}") });
                info!(post_id, "пост сохранён, событие отправлено");
            } else {
                info!("пост не сохранён");
            }
            (result.as_ref().map_or_else(PostError::status, |_| 201), result)
        };
        self.red.track("create", work).instrument(span).await
    }

    async fn save(&self, body: &str) -> Result<u64, PostError> {
        match body {
            "" => Err(PostError::Empty),
            "boom" => Err(PostError::Broken),
            "slow" => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(self.next_post.fetch_add(1, Ordering::SeqCst))
            }
            _ => Ok(self.next_post.fetch_add(1, Ordering::SeqCst)),
        }
    }
}
