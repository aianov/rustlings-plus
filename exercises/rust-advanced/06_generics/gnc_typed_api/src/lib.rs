use std::fmt;
use std::iter::Sum;
use std::marker::PhantomData;
use std::ops::{Add, Mul};
use std::str::FromStr;

pub trait Kind {
    const PREFIX: &'static str;
}

pub struct User;
pub struct Post;

impl Kind for User {
    const PREFIX: &'static str = "u";
}

impl Kind for Post {
    const PREFIX: &'static str = "p";
}

#[derive(Debug, PartialEq)]
pub enum IdError {
    WrongPrefix { expected: &'static str },
    BadNumber(String),
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdError::WrongPrefix { expected } => write!(f, "ожидался префикс {expected}_"),
            IdError::BadNumber(why) => write!(f, "плохой номер: {why}"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct ParamError {
    pub key: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Money(pub u64);

#[derive(Debug, PartialEq)]
pub struct NewPost {
    pub author: Id<User>,
    pub text: String,
    pub boost: Option<Money>,
}

pub struct Missing;
pub struct Set;

pub struct Id<T> {
    raw: u64,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    pub fn new(raw: u64) -> Self {
        todo!()
    }

    pub fn raw(&self) -> u64 {
        todo!()
    }
}

impl<T: Kind> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl<T: Kind> FromStr for Id<T> {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, IdError> {
        todo!()
    }
}

impl Money {
    pub fn checked_sub(self, rhs: Money) -> Option<Money> {
        todo!()
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

pub struct Table<T> {
    rows: Vec<T>,
}

impl<T> Table<T> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn insert(&mut self, row: T) -> Id<T> {
        todo!()
    }

    pub fn get(&self, id: Id<T>) -> Option<&T> {
        todo!()
    }
}

impl<T: Kind> Table<T> {
    pub fn lookup(&self, query: &str, key: &str) -> Result<Option<&T>, ParamError> {
        todo!()
    }
}

pub struct PostDraft<A, T> {
    _marker: PhantomData<(A, T)>,
}

pub fn param<T>(query: &str, key: &str) -> Result<Option<T>, ParamError> {
    todo!()
}
