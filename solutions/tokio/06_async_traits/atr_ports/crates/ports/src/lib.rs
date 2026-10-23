use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use futures::FutureExt;
use futures::future::BoxFuture;

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u32,
    pub author: u32,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RepoError {
    NotFound,
    Conflict,
}

pub type RepoResult<T> = Result<T, RepoError>;

pub trait UserRepo: Send + Sync {
    fn create(&self, name: String) -> BoxFuture<'_, RepoResult<User>>;
    fn get(&self, id: u32) -> BoxFuture<'_, RepoResult<User>>;
    fn count(&self) -> BoxFuture<'_, usize>;
}

pub trait PostRepo: Send + Sync {
    fn add(&self, author: u32, body: String) -> BoxFuture<'_, RepoResult<Post>>;
    fn by_author(&self, author: u32) -> BoxFuture<'_, Vec<Post>>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub user: User,
    pub posts: Vec<Post>,
}

pub struct Service {
    users: Arc<dyn UserRepo>,
    posts: Arc<dyn PostRepo>,
}

impl Service {
    pub fn new(users: Arc<dyn UserRepo>, posts: Arc<dyn PostRepo>) -> Service {
        Service { users, posts }
    }

    pub async fn register(&self, name: &str) -> RepoResult<User> {
        self.users.create(name.to_string()).await
    }

    pub async fn publish(&self, author: u32, body: &str) -> RepoResult<Post> {
        self.users.get(author).await?;
        self.posts.add(author, body.to_string()).await
    }

    pub async fn profile(&self, id: u32) -> RepoResult<Profile> {
        let (user, posts) = tokio::join!(self.users.get(id), self.posts.by_author(id));
        Ok(Profile { user: user?, posts })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    Ok(String),
    Error(String),
    UnknownCommand,
}

type Handler = Box<dyn Fn(Vec<String>) -> BoxFuture<'static, Response> + Send + Sync>;

pub struct Commands {
    handlers: HashMap<String, Handler>,
}

fn failed(error: RepoError) -> Response {
    Response::Error(
        match error {
            RepoError::NotFound => "not found",
            RepoError::Conflict => "conflict",
        }
        .to_string(),
    )
}

async fn register_user(service: &Service, args: Vec<String>) -> Response {
    let [name] = args.as_slice() else {
        return Response::Error("usage: user.register <name>".to_string());
    };
    match service.register(name).await {
        Ok(user) => Response::Ok(user.id.to_string()),
        Err(error) => failed(error),
    }
}

async fn publish_post(service: &Service, args: Vec<String>) -> Response {
    let [author, body] = args.as_slice() else {
        return Response::Error("usage: post.publish <author> <text>".to_string());
    };
    let Ok(author) = author.parse::<u32>() else {
        return Response::Error("bad author id".to_string());
    };
    match service.publish(author, body).await {
        Ok(post) => Response::Ok(post.id.to_string()),
        Err(error) => failed(error),
    }
}

async fn show_profile(service: &Service, args: Vec<String>) -> Response {
    let [id] = args.as_slice() else {
        return Response::Error("usage: user.profile <id>".to_string());
    };
    let Ok(id) = id.parse::<u32>() else {
        return Response::Error("bad user id".to_string());
    };
    match service.profile(id).await {
        Ok(profile) => Response::Ok(format!("{}:{}", profile.user.name, profile.posts.len())),
        Err(error) => failed(error),
    }
}

impl Commands {
    pub fn new(service: Arc<Service>) -> Commands {
        let mut commands = Commands { handlers: HashMap::new() };
        let shared = Arc::clone(&service);
        commands.register("user.register", move |args| {
            let service = Arc::clone(&shared);
            async move { register_user(&service, args).await }
        });
        let shared = Arc::clone(&service);
        commands.register("post.publish", move |args| {
            let service = Arc::clone(&shared);
            async move { publish_post(&service, args).await }
        });
        commands.register("user.profile", move |args| {
            let service = Arc::clone(&service);
            async move { show_profile(&service, args).await }
        });
        commands
    }

    pub fn register<F, Fut>(&mut self, name: &str, handler: F)
    where
        F: Fn(Vec<String>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Response> + Send + 'static,
    {
        self.handlers.insert(name.to_string(), Box::new(move |args| handler(args).boxed()));
    }

    pub async fn dispatch(&self, name: &str, args: Vec<String>) -> Response {
        match self.handlers.get(name) {
            Some(handler) => handler(args).await,
            None => Response::UnknownCommand,
        }
    }
}
