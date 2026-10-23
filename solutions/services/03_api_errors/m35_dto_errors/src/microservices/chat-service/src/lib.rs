pub mod bootstrap;
pub mod client;
pub mod common;
pub mod controller;
pub mod dto;
pub mod repository;
pub mod service;

pub use bootstrap::{run, serve};
pub use client::GrpcUsers;
pub use common::{
    ADMIN_ENV, Config, DEFAULT_ADMIN, Error, JOURNAL_ENV, MAX_MESSAGE_LEN, Message, MessageText,
    NAME, Result,
};
pub use controller::{ChatController, HISTORY_DEFAULT, HISTORY_MAX};
pub use repository::{Line, MemoryChats};
pub use service::{BACKLOG, ChatError, ChatService, Hub, UserDirectory};
