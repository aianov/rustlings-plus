use std::sync::Arc;

use contracts::{MarkedView, NoticeAsk, NotifyMethod, UnreadView};
use nats_engine::{Bus, BusError, Envelope, Server};

use super::pattern;
use crate::common::Config;
use crate::dto;
use crate::service::NotifyService;

pub async fn start(
    bus: &Bus,
    config: &Config,
    service: Arc<NotifyService>,
) -> Result<Vec<Server>, BusError> {
    let topics = &config.topics;
    let group = Some(config.group.as_str());
    let unread = {
        let service = Arc::clone(&service);
        let subject = pattern(&topics.notify(NotifyMethod::Unread))?;
        bus.serve(&subject, group, move |ask: Envelope<NoticeAsk>| {
            let service = Arc::clone(&service);
            async move {
                let unread = service.unread(ask.body.user).await;
                Ok(UnreadView { unread })
            }
        })
        .await?
    };
    let list = {
        let service = Arc::clone(&service);
        let subject = pattern(&topics.notify(NotifyMethod::List))?;
        bus.serve(&subject, group, move |ask: Envelope<NoticeAsk>| {
            let service = Arc::clone(&service);
            async move {
                let limit = dto::limit(&ask.body)?;
                let user = ask.body.user;
                let notices = service.list(user, limit).await;
                let unread = service.unread(user).await;
                Ok(dto::notices(&notices, unread))
            }
        })
        .await?
    };
    let read = {
        let subject = pattern(&topics.notify(NotifyMethod::Read))?;
        bus.serve(&subject, group, move |ask: Envelope<NoticeAsk>| {
            let service = Arc::clone(&service);
            async move {
                let marked = service.mark_read(ask.body.user).await;
                Ok(MarkedView { marked })
            }
        })
        .await?
    };
    Ok(vec![unread, list, read])
}
