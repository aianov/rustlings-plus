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
        Id { raw, _marker: PhantomData }
    }

    pub fn raw(&self) -> u64 {
        self.raw
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for Id<T> {}

impl<T> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.raw)
    }
}

impl<T: Kind> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}", T::PREFIX, self.raw)
    }
}

impl<T: Kind> FromStr for Id<T> {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, IdError> {
        let number = s
            .strip_prefix(T::PREFIX)
            .and_then(|rest| rest.strip_prefix('_'))
            .ok_or(IdError::WrongPrefix { expected: T::PREFIX })?;
        number.parse().map(Id::new).map_err(|e: std::num::ParseIntError| IdError::BadNumber(e.to_string()))
    }
}

impl Add for Money {
    type Output = Money;

    fn add(self, rhs: Money) -> Money {
        Money(self.0 + rhs.0)
    }
}

impl Mul<u32> for Money {
    type Output = Money;

    fn mul(self, rhs: u32) -> Money {
        Money(self.0 * u64::from(rhs))
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money(0), |acc, m| acc + m)
    }
}

impl Money {
    pub fn checked_sub(self, rhs: Money) -> Option<Money> {
        self.0.checked_sub(rhs.0).map(Money)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}", self.0 / 100, self.0 % 100)
    }
}

pub struct Table<T> {
    rows: Vec<T>,
}

impl<T> Table<T> {
    pub fn new() -> Self {
        Table { rows: Vec::new() }
    }

    pub fn insert(&mut self, row: T) -> Id<T> {
        self.rows.push(row);
        Id::new(self.rows.len() as u64)
    }

    pub fn get(&self, id: Id<T>) -> Option<&T> {
        let index = usize::try_from(id.raw()).ok()?.checked_sub(1)?;
        self.rows.get(index)
    }
}

impl<T> Default for Table<T> {
    fn default() -> Self {
        Table::new()
    }
}

impl<T: Kind> Table<T> {
    pub fn lookup(&self, query: &str, key: &str) -> Result<Option<&T>, ParamError> {
        Ok(param::<Id<T>>(query, key)?.and_then(|id| self.get(id)))
    }
}

pub struct PostDraft<A, T> {
    author: Option<Id<User>>,
    text: Option<String>,
    boost: Option<Money>,
    _marker: PhantomData<(A, T)>,
}

impl PostDraft<Missing, Missing> {
    pub fn new() -> Self {
        PostDraft { author: None, text: None, boost: None, _marker: PhantomData }
    }
}

impl Default for PostDraft<Missing, Missing> {
    fn default() -> Self {
        PostDraft::new()
    }
}

impl<A, T> PostDraft<A, T> {
    pub fn author(self, author: Id<User>) -> PostDraft<Set, T> {
        PostDraft { author: Some(author), text: self.text, boost: self.boost, _marker: PhantomData }
    }

    pub fn text(self, text: impl Into<String>) -> PostDraft<A, Set> {
        PostDraft { author: self.author, text: Some(text.into()), boost: self.boost, _marker: PhantomData }
    }

    pub fn boost(mut self, boost: Money) -> Self {
        self.boost = Some(boost);
        self
    }
}

impl PostDraft<Set, Set> {
    pub fn build(self) -> Option<NewPost> {
        Some(NewPost { author: self.author?, text: self.text?, boost: self.boost })
    }
}

pub fn param<T>(query: &str, key: &str) -> Result<Option<T>, ParamError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    let Some(raw) = query.split('&').filter_map(|pair| pair.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| v) else {
        return Ok(None);
    };
    raw.parse().map(Some).map_err(|e: T::Err| ParamError { key: key.to_string(), reason: e.to_string() })
}
