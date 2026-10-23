use std::sync::Arc;

use contracts::{DomainEvent, Fact, Push, Topics, TypingNote, UserId};
use nats_engine::js::{Live, Stored, live};
use nats_engine::{Bus, BusError, Pattern, Subject, Subscription};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::common::{NAME, Registry};

pub(super) struct Subs {
    pub(super) refs: usize,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

pub(super) fn message(user: UserId, stored: &Stored<DomainEvent>) -> Option<Push> {
    let envelope = match &stored.item {
        Ok(envelope) => envelope,
        Err(error) => {
            tracing::warn!(
                "{NAME}: сообщение №{} не разобрано: {error}",
                stored.sequence
            );
            return None;
        }
    };
    match envelope.body.fact() {
        Ok(Some(Fact::MessageSent(sent))) if sent.to_id == user => Some(Push::Message {
            to: sent.to_id,
            from: sent.from_id,
            seq: stored.sequence,
            text: sent.text,
        }),
        Ok(..) => None,
        Err(error) => {
            tracing::warn!(
                "{NAME}: сообщение №{} не прочитано: {error}",
                stored.sequence
            );
            None
        }
    }
}

fn subject(words: &[String]) -> Result<Subject, BusError> {
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    Subject::new(&words).map_err(|error| BusError::Encode(error.to_string()))
}

async fn forward(
    user: UserId,
    registry: Arc<Registry>,
    mut letters: Live<DomainEvent>,
    mut typing: Subscription<TypingNote>,
    mut stopped: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            item = letters.next() => match item {
                Some(stored) => {
                    if let Some(push) = message(user, &stored) {
                        registry.deliver_stored(user, stored.sequence, &push);
                    }
                }
                None => break,
            },
            item = typing.next() => match item {
                Some(Ok(envelope)) if envelope.body.to == user => {
                    let push = Push::Typing { to: user, from: envelope.body.from };
                    registry.deliver(user, &push);
                }
                Some(Ok(..)) => {}
                Some(Err(error)) => tracing::warn!("{NAME}: «печатает» не разобрано: {error}"),
                None => break,
            },
        }
    }
    letters.stop().await;
    typing.stop().await;
}

impl Subs {
    pub(super) async fn open(
        bus: &Bus,
        topics: &Topics,
        user: UserId,
        registry: Arc<Registry>,
    ) -> Result<Subs, BusError> {
        let mut letters = live::<DomainEvent>(bus, &subject(&topics.live(user))?).await?;
        let pattern = Pattern::parse(&topics.typing(user).join("."))
            .map_err(|error| BusError::Encode(error.to_string()));
        let typing = match pattern {
            Ok(pattern) => bus.subscribe::<TypingNote>(&pattern).await,
            Err(error) => Err(error),
        };
        let typing = match typing {
            Ok(typing) => typing,
            Err(error) => {
                letters.stop().await;
                return Err(error);
            }
        };
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(forward(user, registry, letters, typing, stopped));
        Ok(Subs {
            refs: 1,
            stop: Some(stop),
            task,
        })
    }

    pub(super) async fn close(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let _ = self.task.await;
    }
}
