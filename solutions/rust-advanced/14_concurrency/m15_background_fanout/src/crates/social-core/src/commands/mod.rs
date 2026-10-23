mod bus;
mod context;
mod error;
mod response;

use contracts::{COMMAND_NAMES, Command, Username};

use crate::domain::{Post, User, UserId, Visibility, now};
use crate::feed::PagesExt;
use crate::network::Network;
use crate::notify::Notification;
use crate::ports::{PostStore, UserStore};
use crate::shared::SharedNetwork;
use crate::trends::TRENDS_WINDOW;

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
    execute_line(line, ctx.require::<SharedNetwork<U, P>>()?)
}

pub fn execute_line<U: UserStore, P: PostStore>(
    line: &str,
    net: &SharedNetwork<U, P>,
) -> Result<String, CommandError> {
    let command: Command = line.parse()?;
    execute(&command, net)
}

pub fn execute<U: UserStore, P: PostStore>(
    command: &Command,
    net: &SharedNetwork<U, P>,
) -> Result<String, CommandError> {
    match command {
        Command::Register(name) => {
            let id = net.register(name.as_ref())?;
            Ok(format!("зарегистрирован {name} (id {})", id.0))
        }
        Command::Post { author, text } => {
            let author = id_of(net, author)?;
            let post = net.publish(author, text, Visibility::Public)?;
            Ok(format!("опубликован пост [{}]", post.0))
        }
        Command::Follow { who, whom } => {
            let added = net.follow(id_of(net, who)?, id_of(net, whom)?)?;
            if added {
                Ok(format!("{who} подписан на {whom}"))
            } else {
                Ok(format!("{who} уже подписан на {whom}"))
            }
        }
        Command::Like { who, post } => {
            let liked = net.like(id_of(net, who)?, *post)?;
            if liked {
                Ok(format!("{who} лайкнул пост [{}]", post.0))
            } else {
                Ok(format!("{who} уже лайкал пост [{}]", post.0))
            }
        }
        Command::Feed { who, limit } => {
            let viewer = id_of(net, who)?;
            let lines: Vec<String> = net.read(|network| {
                network
                    .feed(viewer)
                    .take(*limit)
                    .map(|post| feed_line(network, post))
                    .collect()
            });
            Ok(join_or(lines, "лента пуста"))
        }
        Command::Page { who, number, size } => {
            let viewer = id_of(net, who)?;
            let page: Option<Vec<String>> = net.read(|network| {
                let page = network
                    .feed(viewer)
                    .pages(*size)
                    .nth(number.checked_sub(1)?)?;
                Some(page.iter().map(|post| feed_line(network, post)).collect())
            });
            match page {
                Some(lines) => Ok(lines.join("\n")),
                None => Ok(format!("страницы {number} нет")),
            }
        }
        Command::Mute { who, whom } => {
            let added = net.mute(id_of(net, who)?, id_of(net, whom)?)?;
            if added {
                Ok(format!("{who} больше не видит {whom} в ленте"))
            } else {
                Ok(format!("{who} уже скрыл {whom}"))
            }
        }
        Command::Delete { who, post } => {
            let deleted = net.delete_post(id_of(net, who)?, *post)?;
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
            let lines = net.read(|network| {
                network
                    .author_page(viewer, author, *after, *size)
                    .map(|page| {
                        let mut lines: Vec<String> = page
                            .posts
                            .iter()
                            .map(|post| feed_line(network, post))
                            .collect();
                        lines.extend(page.next.map(|next| format!("дальше: {next}")));
                        lines
                    })
            })?;
            Ok(join_or(lines, "постов нет"))
        }
        Command::Comment { who, post, text } => {
            let id = net.comment(id_of(net, who)?, *post, text)?;
            Ok(format!("комментарий [{}] к посту [{}]", id.0, post.0))
        }
        Command::Reply { who, to, text } => {
            let id = net.reply(id_of(net, who)?, *to, text)?;
            Ok(format!("ответ [{}] на комментарий [{}]", id.0, to.0))
        }
        Command::Thread { who, post } => {
            let viewer = id_of(net, who)?;
            let lines = net.read(|network| {
                network.thread(viewer, *post).map(|thread| {
                    thread
                        .iter()
                        .map(|(depth, comment)| {
                            let author = name_of(network.user(comment.author()), comment.author());
                            let indent = "  ".repeat(*depth);
                            format!("{indent}[{}] {author}: {}", comment.id().0, comment.text())
                        })
                        .collect::<Vec<String>>()
                })
            })?;
            Ok(join_or(lines, "комментариев пока нет"))
        }
        Command::Trends { limit } => {
            let lines: Vec<String> = net.read(|network| {
                network
                    .trending(now(), TRENDS_WINDOW, *limit)
                    .iter()
                    .filter_map(|trend| {
                        let post = network.post(trend.post)?;
                        Some(format!(
                            "{}, за сутки +{}",
                            feed_line(network, post),
                            trend.likes
                        ))
                    })
                    .collect()
            });
            Ok(join_or(lines, "трендов пока нет"))
        }
        Command::Inbox(who) => {
            let user = id_of(net, who)?;
            let lines: Vec<String> = net
                .inbox(user)
                .iter()
                .map(|notification| describe(net, notification))
                .collect();
            Ok(join_or(lines, "уведомлений нет"))
        }
        Command::Stats => {
            net.flush();
            let stats = net.stats();
            Ok(format!(
                "постов: {}, уведомлений: {}",
                stats.posts, stats.notifications
            ))
        }
        Command::SendMessage { from, to, text } => {
            let message = net.send_message(id_of(net, from)?, id_of(net, to)?, text)?;
            Ok(format!("{from} → {to}: {}", message.text()))
        }
        Command::History { who, with, limit } => {
            let user = id_of(net, who)?;
            let peer = id_of(net, with)?;
            let lines = net.read(|network| {
                network.history(user, peer, *limit).map(|messages| {
                    messages
                        .iter()
                        .map(|message| {
                            let sender = if message.sender() == user { who } else { with };
                            format!("{sender}: {}", message.text())
                        })
                        .collect::<Vec<String>>()
                })
            })?;
            Ok(join_or(lines, "переписки пока нет"))
        }
    }
}

fn id_of<U: UserStore, P: PostStore>(
    net: &SharedNetwork<U, P>,
    name: &Username,
) -> Result<UserId, CommandError> {
    net.read(|network| network.find_user(name.as_ref()).map(User::id))
        .ok_or_else(|| CommandError::UnknownName(name.as_ref().to_string()))
}

fn describe<U: UserStore, P: PostStore>(
    net: &SharedNetwork<U, P>,
    notification: &Notification,
) -> String {
    let name = |id: UserId| name_of(net.profile(id).as_deref(), id);
    match *notification {
        Notification::Followed { by } => format!("{} подписался на тебя", name(by)),
        Notification::Liked { by, post } => {
            format!("{} лайкнул твой пост [{}]", name(by), post.0)
        }
        Notification::Message { from } => format!("{} написал тебе", name(from)),
        Notification::NewPost { author, post } => {
            format!("{} опубликовал пост [{}]", name(author), post.0)
        }
    }
}

fn feed_line<U: UserStore, P: PostStore>(network: &Network<U, P>, post: &Post) -> String {
    let author = name_of(network.user(post.author()), post.author());
    format!("{author} {post}")
}

fn name_of(user: Option<&User>, id: UserId) -> String {
    match user {
        Some(user) => format!("@{}", user.name()),
        None => format!("id {}", id.0),
    }
}

fn join_or(lines: Vec<String>, empty: &str) -> String {
    if lines.is_empty() {
        empty.to_string()
    } else {
        lines.join("\n")
    }
}
