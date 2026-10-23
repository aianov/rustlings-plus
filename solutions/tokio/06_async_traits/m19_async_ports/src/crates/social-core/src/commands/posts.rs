use contracts::Command;

use super::render::Text;
use super::{CommandError, id_of};
use crate::app::App;
use crate::domain::Visibility;

pub async fn create(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Post { author, text } = command else {
        return Err(CommandError::NoRoute("posts.create"));
    };
    let author = id_of(&app, &author).await?;
    let post = app.publish(author, &text, Visibility::Public).await?;
    Ok(format!("опубликован пост [{}]", post.0))
}

pub async fn like(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Like { who, post } = command else {
        return Err(CommandError::NoRoute("posts.like"));
    };
    let liked = app.like(id_of(&app, &who).await?, post).await?;
    if liked {
        Ok(format!("{who} лайкнул пост [{}]", post.0))
    } else {
        Ok(format!("{who} уже лайкал пост [{}]", post.0))
    }
}

pub async fn delete(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Delete { who, post } = command else {
        return Err(CommandError::NoRoute("posts.delete"));
    };
    let deleted = app.delete_post(id_of(&app, &who).await?, post).await?;
    if deleted {
        Ok(format!("пост [{}] удалён", post.0))
    } else {
        Ok(format!("пост [{}] уже удалён", post.0))
    }
}

pub async fn feed(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Feed { who, limit } = command else {
        return Err(CommandError::NoRoute("posts.feed"));
    };
    let viewer = id_of(&app, &who).await?;
    let posts = app.feed(viewer, 0, limit).await;
    let mut text = Text::with_lines(posts.len());
    for post in &posts {
        text.post(post);
    }
    Ok(text.or("лента пуста"))
}

pub async fn page(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Page { who, number, size } = command else {
        return Err(CommandError::NoRoute("posts.page"));
    };
    let viewer = id_of(&app, &who).await?;
    let skip = number
        .checked_sub(1)
        .and_then(|before| before.checked_mul(size));
    let posts = match skip {
        Some(skip) if size > 0 => app.feed(viewer, skip, size).await,
        _ => Vec::new(),
    };
    if posts.is_empty() {
        return Ok(format!("страницы {number} нет"));
    }
    let mut text = Text::with_lines(posts.len());
    for post in &posts {
        text.post(post);
    }
    Ok(text.or(""))
}

pub async fn author(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Posts {
        who,
        author,
        size,
        after,
    } = command
    else {
        return Err(CommandError::NoRoute("posts.author"));
    };
    let viewer = id_of(&app, &who).await?;
    let author = id_of(&app, &author).await?;
    let page = app.author_page(viewer, author, after, size).await?;
    let mut text = Text::with_lines(page.posts.len() + 1);
    for post in &page.posts {
        text.post(post);
    }
    if let Some(next) = page.next {
        text.line(format_args!("дальше: {next}"));
    }
    Ok(text.or("постов нет"))
}

pub async fn trends(app: App, command: Command) -> Result<String, CommandError> {
    let Command::Trends { limit } = command else {
        return Err(CommandError::NoRoute("posts.trends"));
    };
    app.refresh_trends().await?;
    let trends = app.trends(limit);
    let mut text = Text::with_lines(trends.len());
    for trend in &trends {
        if let Some(post) = app.post(trend.post).await {
            text.trend(&post, trend.likes);
        }
    }
    Ok(text.or("трендов пока нет"))
}
