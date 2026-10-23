use contracts::Command;

use super::render::{Name, Text};
use super::{CommandError, id_of};
use crate::app::App;

pub async fn create(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Comment { who, post, text } = command else {
        return Err(CommandError::NoRoute("comments.create"));
    };
    let id = app.comment(id_of(&app, &who).await?, post, &text).await?;
    Ok(format!("комментарий [{}] к посту [{}]", id.0, post.0))
}

pub async fn reply(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Reply { who, to, text } = command else {
        return Err(CommandError::NoRoute("comments.reply"));
    };
    let id = app.reply(id_of(&app, &who).await?, to, &text).await?;
    Ok(format!("ответ [{}] на комментарий [{}]", id.0, to.0))
}

pub async fn thread(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Thread { who, post } = command else {
        return Err(CommandError::NoRoute("comments.thread"));
    };
    let viewer = id_of(&app, &who).await?;
    let thread = app.thread(viewer, post).await?;
    let mut text = Text::with_lines(thread.len());
    for (depth, comment) in &thread {
        let profile = app.profile(comment.author()).await;
        let author = Name::of(profile.as_deref(), comment.author());
        text.line(format_args!(
            "{:indent$}[{}] {author}: {}",
            "",
            comment.id().0,
            comment.text(),
            indent = depth * 2
        ));
    }
    Ok(text.or("комментариев пока нет"))
}
