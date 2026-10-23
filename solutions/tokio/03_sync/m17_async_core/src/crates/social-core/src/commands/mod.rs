mod bus;
mod context;
mod error;
mod render;
mod response;

use contracts::{COMMAND_NAMES, Command, Username};

use crate::app::App;
use crate::domain::{User, UserId, Visibility};
use crate::feed::PagesExt;
use crate::notify::Notification;
use crate::ports::{PostStore, UserStore};
use render::{Name, Text};

pub use bus::CommandBus;
pub use context::Context;
pub use error::CommandError;
pub use response::{IntoResponse, Response};

pub fn network_commands<U, P>(bus: &mut CommandBus)
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    for name in COMMAND_NAMES {
        bus.register(name, run_line::<U, P>);
    }
}

fn run_line<U, P>(ctx: &mut Context, line: &str) -> Result<String, CommandError>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    let app = ctx.require::<App<U, P>>()?;
    app.handle().block_on(execute_line(line, app))
}

pub async fn execute_line<U, P>(line: &str, net: &App<U, P>) -> Result<String, CommandError>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    let command: Command = line.parse()?;
    execute(&command, net).await
}

pub async fn execute<U, P>(command: &Command, net: &App<U, P>) -> Result<String, CommandError>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    match command {
        Command::Register(name) => {
            let id = net.register(name.as_ref()).await?;
            Ok(format!("зарегистрирован {name} (id {})", id.0))
        }
        Command::Post { author, text } => {
            let author = id_of(net, author)?;
            let post = net.publish(author, text, Visibility::Public).await?;
            Ok(format!("опубликован пост [{}]", post.0))
        }
        Command::Follow { who, whom } => {
            let added = net.follow(id_of(net, who)?, id_of(net, whom)?).await?;
            if added {
                Ok(format!("{who} подписан на {whom}"))
            } else {
                Ok(format!("{who} уже подписан на {whom}"))
            }
        }
        Command::Like { who, post } => {
            let liked = net.like(id_of(net, who)?, *post).await?;
            if liked {
                Ok(format!("{who} лайкнул пост [{}]", post.0))
            } else {
                Ok(format!("{who} уже лайкал пост [{}]", post.0))
            }
        }
        Command::Feed { who, limit } => {
            let viewer = id_of(net, who)?;
            Ok(net.read(|network| {
                let mut text = Text::with_lines(*limit);
                for post in network.feed(viewer).take(*limit) {
                    text.post(network, post);
                }
                text.or("лента пуста")
            }))
        }
        Command::Page { who, number, size } => {
            let viewer = id_of(net, who)?;
            let page = net.read(|network| {
                let page = network
                    .feed(viewer)
                    .pages(*size)
                    .nth(number.checked_sub(1)?)?;
                let mut text = Text::with_lines(page.len());
                for post in page {
                    text.post(network, post);
                }
                Some(text.or(""))
            });
            Ok(page.unwrap_or_else(|| format!("страницы {number} нет")))
        }
        Command::Mute { who, whom } => {
            let added = net.mute(id_of(net, who)?, id_of(net, whom)?).await?;
            if added {
                Ok(format!("{who} больше не видит {whom} в ленте"))
            } else {
                Ok(format!("{who} уже скрыл {whom}"))
            }
        }
        Command::Delete { who, post } => {
            let deleted = net.delete_post(id_of(net, who)?, *post).await?;
            if deleted {
                Ok(format!("пост [{}] удалён", post.0))
            } else {
                Ok(format!("пост [{}] уже удалён", post.0))
            }
        }
        Command::Posts {
            who,
            author,
            size,
            after,
        } => {
            let viewer = id_of(net, who)?;
            let author = id_of(net, author)?;
            let text = net.read(|network| {
                network
                    .author_page(viewer, author, *after, *size)
                    .map(|page| {
                        let mut text = Text::with_lines(page.posts.len() + 1);
                        for post in &page.posts {
                            text.post(network, post);
                        }
                        if let Some(next) = page.next {
                            text.line(format_args!("дальше: {next}"));
                        }
                        text.or("постов нет")
                    })
            })?;
            Ok(text)
        }
        Command::Comment { who, post, text } => {
            let id = net.comment(id_of(net, who)?, *post, text).await?;
            Ok(format!("комментарий [{}] к посту [{}]", id.0, post.0))
        }
        Command::Reply { who, to, text } => {
            let id = net.reply(id_of(net, who)?, *to, text).await?;
            Ok(format!("ответ [{}] на комментарий [{}]", id.0, to.0))
        }
        Command::Thread { who, post } => {
            let viewer = id_of(net, who)?;
            let text = net.read(|network| {
                network.thread(viewer, *post).map(|thread| {
                    let mut text = Text::with_lines(thread.len());
                    for (depth, comment) in &thread {
                        let author = Name::new(network.user(comment.author()), comment.author());
                        text.line(format_args!(
                            "{:indent$}[{}] {author}: {}",
                            "",
                            comment.id().0,
                            comment.text(),
                            indent = depth * 2
                        ));
                    }
                    text.or("комментариев пока нет")
                })
            })?;
            Ok(text)
        }
        Command::Trends { limit } => {
            net.refresh_trends().await?;
            let trends = net.trends(*limit);
            Ok(net.read(|network| {
                let mut text = Text::with_lines(trends.len());
                for trend in &trends {
                    if let Some(post) = network.post(trend.post) {
                        let author = Name::new(network.user(post.author()), post.author());
                        text.line(format_args!("{author} {post}, за сутки +{}", trend.likes));
                    }
                }
                text.or("трендов пока нет")
            }))
        }
        Command::Inbox(who) => {
            let user = id_of(net, who)?;
            let inbox = net.inbox(user).await?;
            let mut text = Text::with_lines(inbox.len());
            for notification in inbox {
                describe(&mut text, net, notification);
            }
            Ok(text.or("уведомлений нет"))
        }
        Command::Import { path } => {
            let imported = net.import_file(path).await?;
            Ok(format!(
                "импортировано: пользователей {}, подписок {}, постов {}",
                imported.users, imported.follows, imported.posts
            ))
        }
        Command::Stats => {
            net.flush().await?;
            let stats = net.stats();
            Ok(format!(
                "постов: {}, уведомлений: {}",
                stats.posts, stats.notifications
            ))
        }
        Command::SendMessage { from, to, text } => {
            let message = net
                .send_message(id_of(net, from)?, id_of(net, to)?, text)
                .await?;
            Ok(format!("{from} → {to}: {}", message.text()))
        }
        Command::History { who, with, limit } => {
            let user = id_of(net, who)?;
            let peer = id_of(net, with)?;
            let text = net.read(|network| {
                network.history(user, peer, *limit).map(|messages| {
                    let mut text = Text::with_lines(messages.len());
                    for message in messages {
                        let sender = if message.sender() == user { who } else { with };
                        text.line(format_args!("{sender}: {}", message.text()));
                    }
                    text.or("переписки пока нет")
                })
            })?;
            Ok(text)
        }
    }
}

fn id_of<U, P>(net: &App<U, P>, name: &Username) -> Result<UserId, CommandError>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    net.read(|network| network.find_user(name.as_ref()).map(User::id))
        .ok_or_else(|| CommandError::UnknownName(name.as_ref().to_string()))
}

fn describe<U, P>(text: &mut Text, net: &App<U, P>, notification: Notification)
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    let actor = match notification {
        Notification::Followed { by } | Notification::Liked { by, .. } => by,
        Notification::Message { from } => from,
        Notification::NewPost { author, .. } => author,
    };
    let profile = net.profile(actor);
    let name = Name::new(profile.as_deref(), actor);
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
