use contracts::{
    Call, Changed, ChatHistory, Envelope, FeedPage, MessageView, Outcome, PostView, Reply,
    Response, UserView, decode, decode_call,
};

use crate::app::App;
use crate::chat::Message;
use crate::domain::{DomainError, UserId, Visibility};
use crate::error::{Error, Result};
use crate::ports::PostItem;

pub async fn serve_line(net: &App, line: &str) -> Response<Reply> {
    let envelope: Envelope = match decode(line) {
        Ok(envelope) => envelope,
        Err(error) => {
            return Response {
                id: None,
                outcome: Outcome::Error(Error::from(error).to_body()),
            };
        }
    };
    let served = match decode_call(&envelope) {
        Ok(call) => serve(net, &call).await,
        Err(error) => Err(Error::from(error)),
    };
    let outcome = match served {
        Ok(reply) => Outcome::Ok(reply),
        Err(error) => Outcome::Error(error.to_body()),
    };
    Response {
        id: Some(envelope.id),
        outcome,
    }
}

pub async fn serve(net: &App, call: &Call) -> Result<Reply> {
    match call {
        Call::Register { username } => {
            let id = net.register(username.as_ref()).await?;
            Ok(Reply::User(UserView {
                id,
                username: username.as_ref().to_string(),
            }))
        }
        Call::FindUser { username } => net
            .find_user(username.as_ref())
            .await
            .map(|user| {
                Reply::User(UserView {
                    id: user.id(),
                    username: user.name().to_string(),
                })
            })
            .ok_or_else(|| Error::UnknownName(username.as_ref().to_string())),
        Call::Follow { user, target } => changed(net.follow(*user, *target).await?),
        Call::Mute { user, target } => changed(net.mute(*user, *target).await?),
        Call::CreatePost { user, text } => {
            let id = net.publish(*user, text, Visibility::Public).await?;
            net.post(id)
                .await
                .map(|post| Reply::Post(post_view(&post)))
                .ok_or(Error::Domain(DomainError::UnknownPost(id)))
        }
        Call::Like { user, post } => changed(net.like(*user, *post).await?),
        Call::DeletePost { user, post } => changed(net.delete_post(*user, *post).await?),
        Call::Feed { user, page, size } => {
            let posts = feed_page(net, *user, *page, *size).await?;
            Ok(Reply::Feed(FeedPage { posts }))
        }
        Call::SendMessage { user, peer, text } => {
            let message = net.send_message(*user, *peer, text).await?;
            Ok(Reply::Message(message_view(&message)))
        }
        Call::History { user, peer, limit } => {
            let messages = net.history(*user, *peer, *limit).await?;
            let messages = messages.iter().map(message_view).collect();
            Ok(Reply::History(ChatHistory { messages }))
        }
    }
}

fn changed(changed: bool) -> Result<Reply> {
    Ok(Reply::Changed(Changed { changed }))
}

async fn feed_page(
    net: &App,
    viewer: UserId,
    page: usize,
    size: usize,
) -> std::result::Result<Vec<PostView>, DomainError> {
    if net.user(viewer).await.is_none() {
        return Err(DomainError::UnknownUser(viewer));
    }
    let skip = page
        .checked_sub(1)
        .and_then(|before| before.checked_mul(size));
    let posts = match skip {
        Some(skip) if size > 0 => net.feed(viewer, skip, size).await,
        _ => Vec::new(),
    };
    Ok(posts.iter().map(post_view).collect())
}

fn post_view(post: &PostItem) -> PostView {
    PostView {
        id: post.id,
        author: post.author,
        author_name: post.author_name.to_string(),
        text: post.text.to_string(),
        likes: post.likes,
    }
}

fn message_view(message: &Message) -> MessageView {
    MessageView {
        from: message.sender(),
        to: message.recipient(),
        text: message.text().to_string(),
    }
}
