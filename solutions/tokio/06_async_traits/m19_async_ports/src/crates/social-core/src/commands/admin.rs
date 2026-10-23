use contracts::{Command, Setting};

use super::CommandError;
use crate::app::App;

pub async fn stats(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Stats = command else {
        return Err(CommandError::NoRoute("admin.stats"));
    };
    app.flush().await?;
    let stats = app.stats();
    Ok(format!(
        "постов: {}, уведомлений: {}",
        stats.posts, stats.notifications
    ))
}

pub async fn import(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Import { path } = command else {
        return Err(CommandError::NoRoute("admin.import"));
    };
    let imported = app.import_file(path).await?;
    Ok(format!(
        "импортировано: пользователей {}, подписок {}, постов {}",
        imported.users, imported.follows, imported.posts
    ))
}

pub async fn set(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Set { setting, value } = command else {
        return Err(CommandError::NoRoute("admin.set"));
    };
    let settings = app.set(setting, value)?;
    let now = match setting {
        Setting::PostLimit => settings.post_limit as u64,
        Setting::TrendsWindow => settings.trends_window,
    };
    Ok(format!("{setting} = {now}"))
}
