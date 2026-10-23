use super::Command;
pub const COMMAND_METHODS: [&str; 21] = [
    "users.register",
    "users.follow",
    "users.mute",
    "users.card",
    "posts.create",
    "posts.like",
    "posts.delete",
    "posts.feed",
    "posts.page",
    "posts.author",
    "posts.trends",
    "comments.create",
    "comments.reply",
    "comments.thread",
    "chat.send",
    "chat.history",
    "notify.inbox",
    "admin.stats",
    "admin.import",
    "admin.set",
    "admin.tasks",
];

impl Command {
    pub fn method(&self) -> &'static str {
        match self {
            Command::Register(..) => "users.register",
            Command::Follow { .. } => "users.follow",
            Command::Mute { .. } => "users.mute",
            Command::Card { .. } => "users.card",
            Command::Post { .. } => "posts.create",
            Command::Like { .. } => "posts.like",
            Command::Delete { .. } => "posts.delete",
            Command::Feed { .. } => "posts.feed",
            Command::Page { .. } => "posts.page",
            Command::Posts { .. } => "posts.author",
            Command::Trends { .. } => "posts.trends",
            Command::Comment { .. } => "comments.create",
            Command::Reply { .. } => "comments.reply",
            Command::Thread { .. } => "comments.thread",
            Command::SendMessage { .. } => "chat.send",
            Command::History { .. } => "chat.history",
            Command::Inbox(..) => "notify.inbox",
            Command::Stats => "admin.stats",
            Command::Import { .. } => "admin.import",
            Command::Set { .. } => "admin.set",
            Command::Tasks => "admin.tasks",
        }
    }
}
