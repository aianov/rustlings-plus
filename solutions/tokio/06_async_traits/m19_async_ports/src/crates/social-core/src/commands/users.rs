use contracts::Command;

use super::render::{self, Text};
use super::{CommandError, id_of};
use crate::app::App;

pub async fn register(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Register(name) = command else {
        return Err(CommandError::NoRoute("users.register"));
    };
    let id = app.register(name.as_ref()).await?;
    Ok(format!("зарегистрирован {name} (id {})", id.0))
}

pub async fn follow(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Follow { who, whom } = command else {
        return Err(CommandError::NoRoute("users.follow"));
    };
    let added = app
        .follow(id_of(&app, &who).await?, id_of(&app, &whom).await?)
        .await?;
    if added {
        Ok(format!("{who} подписан на {whom}"))
    } else {
        Ok(format!("{who} уже подписан на {whom}"))
    }
}

pub async fn mute(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Mute { who, whom } = command else {
        return Err(CommandError::NoRoute("users.mute"));
    };
    let added = app
        .mute(id_of(&app, &who).await?, id_of(&app, &whom).await?)
        .await?;
    if added {
        Ok(format!("{who} больше не видит {whom} в ленте"))
    } else {
        Ok(format!("{who} уже скрыл {whom}"))
    }
}

pub async fn card(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Card { who, whose } = command else {
        return Err(CommandError::NoRoute("users.card"));
    };
    let viewer = id_of(&app, &who).await?;
    let card = app.card(viewer, id_of(&app, &whose).await?).await?;
    let mut posts = Vec::with_capacity(card.recent.len());
    for id in &card.recent {
        posts.extend(app.post(*id).await);
    }
    let mut text = Text::with_lines(posts.len() + 3);
    render::card(&mut text, viewer, &card, &posts);
    Ok(text.or(""))
}
