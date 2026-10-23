use contracts::{Call, ChatHistory, Envelope, MessageView, Push, Reply, UserId, UserView};
use proto::social::v1::GetProfileRequest;
use social_core::Message;

use super::{Answer, Ctx, acting, misrouted};

const USERS: &str = "сервис профилей";

fn view(message: &Message) -> MessageView {
    MessageView {
        from: message.sender(),
        to: message.recipient(),
        text: message.text().to_string(),
    }
}

async fn known(ctx: &Ctx, peer: UserId) -> Result<(), contracts::ErrorBody> {
    let ask = GetProfileRequest { user_id: peer.0 };
    let found = ctx.users().get_profile(ctx.traced(ask)).await;
    found.map(drop).map_err(ctx.failed(USERS))
}

pub(super) async fn send(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = acting(&me, envelope)?;
    let Call::SendMessage { user, peer, text } = call else {
        return Err(misrouted(&call));
    };
    known(&ctx, peer).await?;
    let sent = ctx.chats().send(user, peer, &text);
    let message = sent.map_err(|error| social_core::Error::from(error).to_body())?;
    ctx.push(
        peer,
        &Push::Message {
            to: peer,
            from: user,
        },
    );
    Ok(Reply::Message(view(&message)))
}

pub(super) async fn history(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = acting(&me, envelope)?;
    let Call::History { user, peer, limit } = call else {
        return Err(misrouted(&call));
    };
    known(&ctx, peer).await?;
    let messages = ctx.chats().history(user, peer, limit);
    Ok(Reply::History(ChatHistory {
        messages: messages.iter().map(view).collect(),
    }))
}
