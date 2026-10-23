use contracts::{ChatHistory, ErrorBody, MessageView};
use proto::social::v1::{HistoryRequest, SendRequest};

use super::Ctx;
use crate::common::Principal;
use crate::dto::{HistoryAsk, Letter, message_view as view};

const CHAT: &str = "сервис сообщений";

pub(super) async fn send(ctx: Ctx, _: Principal, data: Letter) -> Result<MessageView, ErrorBody> {
    let ask = SendRequest {
        peer_id: data.peer.0,
        text: data.text,
    };
    let sent = ctx.chat().send(ask).await;
    Ok(view(sent.map_err(ctx.failed(CHAT))?.into_inner()))
}

pub(super) async fn history(
    ctx: Ctx,
    _: Principal,
    data: HistoryAsk,
) -> Result<ChatHistory, ErrorBody> {
    let ask = HistoryRequest {
        peer_id: data.peer.0,
        limit: u32::try_from(data.limit).unwrap_or(u32::MAX),
    };
    let found = ctx.chat().history(ask).await;
    let messages = found.map_err(ctx.failed(CHAT))?.into_inner().messages;
    Ok(ChatHistory {
        messages: messages.into_iter().map(view).collect(),
    })
}
