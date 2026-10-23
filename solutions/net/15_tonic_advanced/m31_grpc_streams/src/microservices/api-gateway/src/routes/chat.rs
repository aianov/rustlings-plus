use contracts::{Call, ChatHistory, Envelope, MessageView, Reply, UserId, UserView};
use proto::social::v1::{HistoryRequest, Message, SendRequest};

use super::{Answer, Ctx, acting, misrouted};

const CHAT: &str = "сервис сообщений";

fn view(message: Message) -> MessageView {
    MessageView {
        from: UserId(message.from_id),
        to: UserId(message.to_id),
        text: message.text,
    }
}

pub(super) async fn send(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = acting(&me, envelope)?;
    let Call::SendMessage { peer, text, .. } = call else {
        return Err(misrouted(call.method()));
    };
    let ask = SendRequest {
        peer_id: peer.0,
        text,
    };
    let sent = ctx.chat().send(ask).await;
    let message = sent.map_err(ctx.failed(CHAT))?.into_inner();
    Ok(Reply::Message(view(message)))
}

pub(super) async fn history(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = acting(&me, envelope)?;
    let Call::History { peer, limit, .. } = call else {
        return Err(misrouted(call.method()));
    };
    let ask = HistoryRequest {
        peer_id: peer.0,
        limit: u32::try_from(limit).unwrap_or(u32::MAX),
    };
    let found = ctx.chat().history(ask).await;
    let messages = found.map_err(ctx.failed(CHAT))?.into_inner().messages;
    Ok(Reply::History(ChatHistory {
        messages: messages.into_iter().map(view).collect(),
    }))
}
