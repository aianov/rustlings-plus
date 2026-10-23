//! Фоновая рассылка уведомлений.
//!
//! TODO: Реализуй `Fanout` по README. Готовый пул лежит в `pool.rs`. Тебе понадобятся:
//! реестр пользователей и логинов под одним замком (проверка и вставка вместе), кэш профилей на `RwLock`,
//! у каждого пользователя свой список друзей под своим `Mutex`, атомарные счётчики в `Arc`.

#[allow(dead_code)]
mod pool;

use std::sync::Arc;

pub type UserId = u32;

pub trait ProfileStore: Send + Sync {
    fn load(&self, id: UserId) -> Option<String>;
}

pub trait Sink: Send + Sync {
    fn deliver(&self, to: UserId, text: &str) -> Result<(), String>;
}

#[derive(Debug, PartialEq)]
pub enum RegisterError {
    Invalid,
    Taken,
}

#[derive(Debug, PartialEq)]
pub enum FollowError {
    SameUser,
    UnknownUser(UserId),
}

#[derive(Debug, PartialEq)]
pub enum PublishError {
    UnknownUser,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    pub posts: u64,
    pub queued: u64,
    pub delivered: u64,
    pub failed: u64,
}

pub struct Fanout {}

impl Fanout {
    pub fn new(workers: usize, store: Arc<dyn ProfileStore>, sink: Arc<dyn Sink>) -> Fanout {
        todo!("сервис с пулом на workers потоков")
    }

    pub fn register(&self, login: &str) -> Result<UserId, RegisterError> {
        todo!("проверка логина и выдача id одной неделимой операцией")
    }

    pub fn profile(&self, id: UserId) -> Option<Arc<String>> {
        todo!("кэш на RwLock, загрузка без замка")
    }

    pub fn befriend(&self, a: UserId, b: UserId) -> Result<(), FollowError> {
        todo!("взаимная дружба под двумя замками в едином порядке")
    }

    pub fn friends(&self, id: UserId) -> Vec<UserId> {
        todo!("друзья по возрастанию")
    }

    pub fn publish(&self, author: UserId, text: &str) -> Result<usize, PublishError> {
        todo!("по заданию в пул на каждого друга")
    }

    pub fn stats(&self) -> Stats {
        todo!("снимок счётчиков")
    }

    pub fn shutdown(&self) -> Stats {
        todo!("закрыть публикации, дождаться отправки, вернуть итог")
    }
}
