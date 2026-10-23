use std::collections::HashMap;
use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode};
use tokio::sync::Semaphore;
use tokio::task::{Id, JoinError, JoinSet};
use tokio::time::Instant;
use tracing::Instrument;

use crate::common::Visitor;
use crate::controller::observe::observe_ws;
use crate::controller::protocol;
use crate::routes::{Answer, Ctx, Router};

struct Owner {
    id: Option<u64>,
    rid: String,
    method: String,
}

pub(super) struct Work {
    tasks: JoinSet<Answer>,
    owners: HashMap<Id, Owner>,
    permits: Arc<Semaphore>,
}

impl Work {
    pub(super) fn new(max_in_flight: usize) -> Work {
        Work {
            tasks: JoinSet::new(),
            owners: HashMap::new(),
            permits: Arc::new(Semaphore::new(max_in_flight.max(1))),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub(super) fn start(
        &mut self,
        router: &Router,
        ctx: &Ctx,
        visitor: &Visitor,
        text: &str,
    ) -> Option<String> {
        if text.trim().is_empty() {
            return None;
        }
        let envelope = match protocol::request(text) {
            Ok(envelope) => envelope,
            Err(reply) => return Some(reply),
        };
        let id = Some(envelope.id);
        let request = ctx.for_request(visitor, &envelope.meta);
        let rid = request.request_id().to_string();
        let Ok(permit) = Arc::clone(&self.permits).try_acquire_owned() else {
            let busy = ErrorBody::public(ErrorCode::Exhausted);
            return Some(protocol::answer(id, Some(&rid), Err(busy)));
        };
        let method = envelope.method.clone();
        let trace = request.trace();
        let span = tracing::info_span!(
            "ws",
            method = %method,
            request_id = %rid,
            trace_id = %trace.trace_id,
        );
        let pending = router.dispatch(request, visitor, envelope);
        let measured = method.clone();
        let handle = self.tasks.spawn(
            async move {
                let _permit = permit;
                let started = Instant::now();
                let answer = service_runtime::with_trace(trace, pending).await;
                let outcome = match &answer {
                    Ok(..) => "ok",
                    Err(..) => "error",
                };
                observe_ws(&measured, outcome, started.elapsed());
                answer
            }
            .instrument(span),
        );
        self.owners.insert(handle.id(), Owner { id, rid, method });
        None
    }

    pub(super) async fn next(&mut self) -> Option<String> {
        let done = self.tasks.join_next_with_id().await?;
        Some(self.finished(done))
    }

    fn finished(&mut self, done: Result<(Id, Answer), JoinError>) -> String {
        let (task, answer) = match done {
            Ok((task, answer)) => (task, answer),
            Err(error) => (error.id(), Err(self.crashed(&error))),
        };
        match self.owners.remove(&task) {
            Some(owner) => protocol::answer(owner.id, Some(&owner.rid), answer),
            None => protocol::answer(None, None, answer),
        }
    }

    fn crashed(&self, error: &JoinError) -> ErrorBody {
        let (rid, method) = self.owners.get(&error.id()).map_or(("-", "?"), |owner| {
            (owner.rid.as_str(), owner.method.as_str())
        });
        tracing::warn!("api-gateway [{rid}]: обработчик {method} упал: {error}");
        ErrorBody::public(ErrorCode::Internal)
    }

    pub(super) fn abort(&mut self) {
        self.tasks.abort_all();
    }
}
