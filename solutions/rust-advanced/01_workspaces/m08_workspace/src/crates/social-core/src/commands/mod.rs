mod error;

use contracts::{Command, Username};

use crate::domain::{User, UserId, Visibility};
use crate::notify::Notification;
use crate::ports::{PostStore, UserStore};
use crate::shared::SharedNetwork;

pub use error::CommandError;

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
                    .feed(viewer, *limit)
                    .iter()
                    .map(|post| {
                        let author = name_of(network.user(post.author()), post.author());
                        format!("{author} {post}")
                    })
                    .collect()
            });
            Ok(join_or(lines, "лента пуста"))
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
    match *notification {
        Notification::Followed { by } => {
            let by = net.read(|network| name_of(network.user(by), by));
            format!("{by} подписался на тебя")
        }
        Notification::Liked { by, post } => {
            let by = net.read(|network| name_of(network.user(by), by));
            format!("{by} лайкнул твой пост [{}]", post.0)
        }
    }
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
