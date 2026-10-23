pub mod bootstrap;
pub mod client;
pub mod common;
pub mod controller;
pub mod dto;
pub mod repository;
pub mod service;

pub use bootstrap::{GATEWAY, run, serve};
pub use client::GrpcUsers;
pub use common::{
    ADMIN_ENV, CommentText, Config, DEFAULT_ADMIN, Draft, Error, JOURNAL_ENV, Link, MAX_LINKS,
    MAX_TAGS, NAME, PostText, Result, Tag,
};
pub use controller::{PostsController, RECENT_MAX, SYNC_BUDGET};
pub use repository::MemoryPosts;
pub use service::{PostError, PostService, UserDirectory};
