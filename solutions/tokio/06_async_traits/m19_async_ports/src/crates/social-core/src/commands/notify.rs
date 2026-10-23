use contracts::Command;

use super::render::{Name, Text};
use super::{CommandError, id_of};
use crate::app::App;
use crate::notify::Notification;

pub async fn inbox(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Inbox(who) = command else {
        return Err(CommandError::NoRoute("notify.inbox"));
    };
    let user = id_of(&app, &who).await?;
    let inbox = app.inbox(user).await?;
    let mut text = Text::with_lines(inbox.len());
    for notification in inbox {
        describe(&mut text, &app, notification).await;
    }
    Ok(text.or("уведомлений нет"))
}

async fn describe(text: &mut Text, app: &App, notification: Notification) {
    let actor = match notification {
        Notification::Followed { by } | Notification::Liked { by, .. } => by,
        Notification::Message { from } => from,
        Notification::NewPost { author, .. } => author,
    };
    let profile = app.profile(actor).await;
    let name = Name::of(profile.as_deref(), actor);
    match notification {
        Notification::Followed { .. } => text.line(format_args!("{name} подписался на тебя")),
        Notification::Liked { post, .. } => {
            text.line(format_args!("{name} лайкнул твой пост [{}]", post.0));
        }
        Notification::Message { .. } => text.line(format_args!("{name} написал тебе")),
        Notification::NewPost { post, .. } => {
            text.line(format_args!("{name} опубликовал пост [{}]", post.0));
        }
    }
}
