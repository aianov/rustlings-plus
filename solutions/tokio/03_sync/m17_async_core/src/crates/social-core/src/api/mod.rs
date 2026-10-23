use contracts::{
    Call, Changed, ChatHistory, Envelope, FeedPage, MessageView, Outcome, PostView, Reply,
    Response, UserView, decode, decode_call,
};

use crate::app::App;
use crate::chat::Message;
use crate::domain::{DomainError, Post, UserId, Visibility};
use crate::error::{Error, Result};
use crate::feed::PagesExt;
use crate::network::Network;
use crate::ports::{PostStore, UserStore};

pub async fn serve_line<U, P>(net: &App<U, P>, line: &str) -> Response<Reply>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
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

pub async fn serve<U, P>(net: &App<U, P>, call: &Call) -> Result<Reply>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    match call {
        Call::Register { username } => {
            let id = net.register(username.as_ref()).await?;
            Ok(Reply::User(UserView {
                id,
                username: username.as_ref().to_string(),
            }))
        }
        Call::FindUser { username } => net
            .read(|network| {
                network.find_user(username.as_ref()).map(|user| UserView {
                    id: user.id(),
                    username: user.name().to_string(),
                })
            })
            .map(Reply::User)
            .ok_or_else(|| Error::UnknownName(username.as_ref().to_string())),
        Call::Follow { user, target } => changed(net.follow(*user, *target).await?),
        Call::Mute { user, target } => changed(net.mute(*user, *target).await?),
        Call::CreatePost { user, text } => {
            let id = net.publish(*user, text, Visibility::Public).await?;
            net.read(|network| network.post(id).map(|post| post_view(network, post)))
                .map(Reply::Post)
                .ok_or(Error::Domain(DomainError::UnknownPost(id)))
        }
        Call::Like { user, post } => changed(net.like(*user, *post).await?),
        Call::DeletePost { user, post } => changed(net.delete_post(*user, *post).await?),
        Call::Feed { user, page, size } => {
            let posts = net.read(|network| feed_page(network, *user, *page, *size))?;
            Ok(Reply::Feed(FeedPage { posts }))
        }
        Call::SendMessage { user, peer, text } => {
            let message = net.send_message(*user, *peer, text).await?;
            Ok(Reply::Message(message_view(&message)))
        }
        Call::History { user, peer, limit } => {
            let messages = net.read(|network| {
                network
                    .history(*user, *peer, *limit)
                    .map(|messages| messages.iter().map(message_view).collect())
            })?;
            Ok(Reply::History(ChatHistory { messages }))
        }
    }
}

fn changed(changed: bool) -> Result<Reply> {
    Ok(Reply::Changed(Changed { changed }))
}

fn feed_page<U: UserStore, P: PostStore>(
    network: &Network<U, P>,
    viewer: UserId,
    page: usize,
    size: usize,
) -> std::result::Result<Vec<PostView>, DomainError> {
    if network.user(viewer).is_none() {
        return Err(DomainError::UnknownUser(viewer));
    }
    let posts = page
        .checked_sub(1)
        .and_then(|skip| network.feed(viewer).pages(size).nth(skip))
        .unwrap_or_default();
    Ok(posts
        .into_iter()
        .map(|post| post_view(network, post))
        .collect())
}

fn post_view<U: UserStore, P: PostStore>(network: &Network<U, P>, post: &Post) -> PostView {
    PostView {
        id: post.id(),
        author: post.author(),
        author_name: network
            .user(post.author())
            .map(|user| user.name().to_string())
            .unwrap_or_default(),
        text: post.text().to_string(),
        likes: post.likes(),
    }
}

fn message_view(message: &Message) -> MessageView {
    MessageView {
        from: message.sender(),
        to: message.recipient(),
        text: message.text().to_string(),
    }
}
