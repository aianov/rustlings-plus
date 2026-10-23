use contracts::{Changed, ChatHistory, ChatLineView, ChatsView, ErrorBody, MessageView, UserId};
use proto::social::v1::{
    ChatsRequest, EditMessageRequest, HistoryRequest, PeerRequest, SendRequest, TypingRequest,
};

use super::Ctx;
use crate::common::Principal;
use crate::dto::{EditAsk, HistoryAsk, Letter, PeerAsk, TopAsk, message_view as view};

const CHAT: &str = "сервис сообщений";

pub(super) async fn send(ctx: Ctx, _: Principal, data: Letter) -> Result<MessageView, ErrorBody> {
    let ask = SendRequest {
        peer_id: data.peer.0,
        text: data.text,
        ttl_seconds: data.ttl.unwrap_or(0),
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
        cursor: data.cursor.unwrap_or_default(),
    };
    let found = ctx.chat().history(ask).await;
    let reply = found.map_err(ctx.failed(CHAT))?.into_inner();
    Ok(ChatHistory {
        messages: reply.messages.into_iter().map(view).collect(),
        next: Some(reply.next).filter(|next| !next.is_empty()),
    })
}

pub(super) async fn list(ctx: Ctx, _: Principal, data: TopAsk) -> Result<ChatsView, ErrorBody> {
    let found = ctx.chat().chats(ChatsRequest { limit: data.limit }).await;
    let reply = found.map_err(ctx.failed(CHAT))?.into_inner();
    Ok(ChatsView {
        chats: reply
            .chats
            .into_iter()
            .map(|chat| ChatLineView {
                peer: UserId(chat.peer_id),
                last_at: chat.last_at,
                preview: chat.preview,
                unread: chat.unread,
            })
            .collect(),
    })
}

pub(super) async fn typing(ctx: Ctx, _: Principal, data: PeerAsk) -> Result<Changed, ErrorBody> {
    let ask = TypingRequest {
        peer_id: data.peer.0,
    };
    let sent = ctx.chat().typing(ask).await;
    let changed = sent.map_err(ctx.failed(CHAT))?.into_inner().sent;
    Ok(Changed { changed })
}

pub(super) async fn edit(ctx: Ctx, _: Principal, data: EditAsk) -> Result<Changed, ErrorBody> {
    let ask = EditMessageRequest {
        peer_id: data.peer.0,
        message_id: data.id,
        text: data.text,
    };
    let edited = ctx.chat().edit_message(ask).await;
    let changed = edited.map_err(ctx.failed(CHAT))?.into_inner().changed;
    Ok(Changed { changed })
}

pub(super) async fn seen(ctx: Ctx, _: Principal, data: PeerAsk) -> Result<Changed, ErrorBody> {
    let marked = ctx
        .chat()
        .mark_read(PeerRequest {
            peer_id: data.peer.0,
        })
        .await;
    let changed = marked.map_err(ctx.failed(CHAT))?.into_inner().changed;
    Ok(Changed { changed })
}
