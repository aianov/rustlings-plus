use std::collections::HashMap;
use std::future::Future;

use contracts::Command;

use super::{CommandError, admin, chat, comments, notify, posts, users};
use crate::app::App;
use crate::ports::BoxFuture;

pub type Reply = Result<String, CommandError>;

pub type Handler = Box<dyn Fn(App, Command) -> BoxFuture<'static, Reply> + Send + Sync>;

#[derive(Default)]
pub struct CommandTable {
    handlers: HashMap<&'static str, Handler>,
}

impl CommandTable {
    pub fn new() -> CommandTable {
        CommandTable::default()
    }

    pub fn route<F, Fut>(&mut self, method: &'static str, handler: F) -> bool
    where
        F: Fn(App, Command) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Reply> + Send + 'static,
    {
        let boxed: Handler = Box::new(move |app, command| Box::pin(handler(app, command)));
        self.handlers.insert(method, boxed).is_none()
    }

    pub fn methods(&self) -> Vec<&'static str> {
        let mut methods: Vec<&'static str> = self.handlers.keys().copied().collect();
        methods.sort_unstable();
        methods
    }

    pub async fn call(&self, app: &App, command: Command) -> Reply {
        let method = command.method();
        let Some(handler) = self.handlers.get(method) else {
            return Err(CommandError::NoRoute(method));
        };
        handler(app.clone(), command).await
    }

    pub fn social() -> CommandTable {
        let mut table = CommandTable::new();
        table.route("users.register", users::register);
        table.route("users.follow", users::follow);
        table.route("users.mute", users::mute);
        table.route("users.card", users::card);
        table.route("posts.create", posts::create);
        table.route("posts.like", posts::like);
        table.route("posts.delete", posts::delete);
        table.route("posts.feed", posts::feed);
        table.route("posts.page", posts::page);
        table.route("posts.author", posts::author);
        table.route("posts.trends", posts::trends);
        table.route("comments.create", comments::create);
        table.route("comments.reply", comments::reply);
        table.route("comments.thread", comments::thread);
        table.route("chat.send", chat::send);
        table.route("chat.history", chat::history);
        table.route("notify.inbox", notify::inbox);
        table.route("admin.stats", admin::stats);
        table.route("admin.import", admin::import);
        table.route("admin.set", admin::set);
        table
    }
}
