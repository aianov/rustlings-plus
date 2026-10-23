mod admin;
mod chat;
mod comments;
mod error;
mod notify;
mod posts;
mod render;
mod response;
mod table;
mod users;

use std::sync::OnceLock;

use contracts::{Command, Username};

use crate::app::App;
use crate::domain::UserId;

pub use error::CommandError;
pub use response::{IntoResponse, Response};
pub use table::{CommandTable, Handler};

pub fn social_table() -> &'static CommandTable {
    static TABLE: OnceLock<CommandTable> = OnceLock::new();
    TABLE.get_or_init(CommandTable::social)
}

pub async fn execute_line(line: &str, app: &App) -> Result<String, CommandError> {
    let command: Command = line.parse()?;
    social_table().call(app, command).await
}

pub async fn execute(command: &Command, app: &App) -> Result<String, CommandError> {
    social_table().call(app, command.clone()).await
}

async fn id_of(app: &App, name: &Username) -> Result<UserId, CommandError> {
    app.find_user(name.as_ref())
        .await
        .map(|user| user.id())
        .ok_or_else(|| CommandError::UnknownName(name.as_ref().to_string()))
}
