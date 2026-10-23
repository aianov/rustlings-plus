use contracts::Command;

use super::render::Text;
use super::{CommandError, id_of};
use crate::app::App;

pub async fn send(app: App, command: Command) -> Result<String, CommandError> {
    let Command::SendMessage { from, to, text } = command else {
        return Err(CommandError::NoRoute("chat.send"));
    };
    let sender = id_of(&app, &from).await?;
    let recipient = id_of(&app, &to).await?;
    let message = app.send_message(sender, recipient, &text).await?;
    Ok(format!("{from} → {to}: {}", message.text()))
}

pub async fn history(app: App, command: Command) -> Result<String, CommandError> {
    let Command::History { who, with, limit } = command else {
        return Err(CommandError::NoRoute("chat.history"));
    };
    let user = id_of(&app, &who).await?;
    let peer = id_of(&app, &with).await?;
    let messages = app.history(user, peer, limit).await?;
    let mut text = Text::with_lines(messages.len());
    for message in &messages {
        let sender = if message.sender() == user {
            &who
        } else {
            &with
        };
        text.line(format_args!("{sender}: {}", message.text()));
    }
    Ok(text.or("переписки пока нет"))
}
