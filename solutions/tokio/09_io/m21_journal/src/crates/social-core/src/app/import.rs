use std::collections::HashMap;
use std::path::PathBuf;

use contracts::{Command, Username};
use tokio::fs;
use tokio::task::JoinSet;

use super::{App, AppError, ImportError, Op};
use crate::domain::{User, UserId, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Imported {
    pub users: usize,
    pub follows: usize,
    pub posts: usize,
}

enum Record {
    User(Username),
    Follow {
        who: Username,
        whom: Username,
    },
    Post {
        author: Username,
        text: String,
    },
    Posts {
        author: Username,
        posts: Vec<(usize, String)>,
    },
}

#[derive(Clone, Copy)]
enum Done {
    User,
    Follow,
    Posts(usize),
}

impl Imported {
    fn count(&mut self, done: Done) {
        match done {
            Done::User => self.users += 1,
            Done::Follow => self.follows += 1,
            Done::Posts(count) => self.posts += count,
        }
    }
}

fn by_author(records: Vec<(usize, Record)>) -> Vec<(usize, Record)> {
    let mut groups: Vec<(usize, Record)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (line, record) in records {
        let Record::Post { author, text } = record else {
            continue;
        };
        let slot = *index.entry(author.as_ref().to_string()).or_insert_with(|| {
            groups.push((
                line,
                Record::Posts {
                    author: author.clone(),
                    posts: Vec::new(),
                },
            ));
            groups.len() - 1
        });
        if let Some((_, Record::Posts { posts, .. })) = groups.get_mut(slot) {
            posts.push((line, text));
        }
    }
    groups
}

fn parse(text: &str) -> Result<Vec<(usize, Record)>, ImportError> {
    let mut records = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        if raw.trim().is_empty() {
            continue;
        }
        let command: Command = raw
            .parse()
            .map_err(|error| ImportError::Parse { line, error })?;
        let record = match command {
            Command::Register(name) => Record::User(name),
            Command::Follow { who, whom } => Record::Follow { who, whom },
            Command::Post { author, text } => Record::Post { author, text },
            _ => return Err(ImportError::Unsupported { line }),
        };
        records.push((line, record));
    }
    Ok(records)
}

impl App {
    pub async fn import(&self, text: String) -> Result<Imported, AppError> {
        let records = self.run_heavy(Op::Import, move || parse(&text)).await??;
        self.import_records(records).await
    }

    pub async fn import_file(&self, path: impl Into<PathBuf>) -> Result<Imported, AppError> {
        let path = path.into();
        let text = fs::read_to_string(&path)
            .await
            .map_err(|error| ImportError::Read {
                kind: error.kind(),
                path,
            })?;
        let records = self.run_heavy(Op::Import, move || parse(&text)).await??;
        self.import_records(records).await
    }

    async fn import_records(&self, records: Vec<(usize, Record)>) -> Result<Imported, AppError> {
        let (users, rest): (Vec<_>, Vec<_>) = records
            .into_iter()
            .partition(|(_, record)| matches!(record, Record::User(..)));
        let (follows, posts): (Vec<_>, Vec<_>) = rest
            .into_iter()
            .partition(|(_, record)| matches!(record, Record::Follow { .. }));
        let mut imported = Imported::default();
        self.run_window(users, &mut imported).await?;
        self.run_window(follows, &mut imported).await?;
        self.run_window(by_author(posts), &mut imported).await?;
        Ok(imported)
    }

    async fn run_window(
        &self,
        records: Vec<(usize, Record)>,
        imported: &mut Imported,
    ) -> Result<(), AppError> {
        let window = self.inner.config.import_window.max(1);
        let mut waiting = records.into_iter();
        let mut running = JoinSet::new();
        let mut lines = HashMap::new();
        loop {
            while running.len() < window {
                let Some((line, record)) = waiting.next() else {
                    break;
                };
                let app = self.clone();
                let task = running.spawn_on(
                    async move { app.apply(line, record).await },
                    &self.inner.runtime,
                );
                lines.insert(task.id(), line);
            }
            let Some(joined) = running.join_next().await else {
                return Ok(());
            };
            match joined {
                Ok(done) => imported.count(done?),
                Err(error) => {
                    let line = lines.get(&error.id()).copied().unwrap_or_default();
                    return Err(ImportError::Panicked { line }.into());
                }
            }
        }
    }

    async fn apply(&self, line: usize, record: Record) -> Result<Done, AppError> {
        let failed = |error| AppError::from(ImportError::Record { line, error });
        match record {
            Record::User(name) => match self.register(name.as_ref()).await {
                Ok(_) => Ok(Done::User),
                Err(AppError::Domain(error)) => Err(failed(error)),
                Err(error) => Err(error),
            },
            Record::Follow { who, whom } => {
                let who = self.id_in(line, &who).await?;
                let whom = self.id_in(line, &whom).await?;
                match self.follow(who, whom).await {
                    Ok(_) => Ok(Done::Follow),
                    Err(AppError::Domain(error)) => Err(failed(error)),
                    Err(error) => Err(error),
                }
            }
            Record::Post { author, text } => self.apply_posts(author, vec![(line, text)]).await,
            Record::Posts { author, posts } => self.apply_posts(author, posts).await,
        }
    }

    async fn apply_posts(
        &self,
        author: Username,
        posts: Vec<(usize, String)>,
    ) -> Result<Done, AppError> {
        let count = posts.len();
        let first = posts.first().map_or(0, |(line, _)| *line);
        let author = self.id_in(first, &author).await?;
        for (line, text) in posts {
            match self.publish(author, &text, Visibility::Public).await {
                Ok(_) => {}
                Err(AppError::Domain(error)) => {
                    return Err(ImportError::Record { line, error }.into());
                }
                Err(error) => return Err(error),
            }
        }
        Ok(Done::Posts(count))
    }

    async fn id_in(&self, line: usize, name: &Username) -> Result<UserId, ImportError> {
        self.find_user(name.as_ref())
            .await
            .map(|user| User::id(&user))
            .ok_or_else(|| ImportError::UnknownName {
                line,
                name: name.as_ref().to_string(),
            })
    }
}
