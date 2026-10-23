use std::sync::Arc;
use std::time::{Duration, Instant};

use contracts::{Login, UserView, decode};
use serde_json::Value;
use social_cli::{Web, Ws};
use test_support::{PASSWORD, unique_name};
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::model::{Hit, Sample};

pub const STEP_LIMIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Login,
    Post,
    Feed,
    Message,
}

impl Step {
    pub fn name(self) -> &'static str {
        match self {
            Step::Login => "login",
            Step::Post => "post",
            Step::Feed => "feed",
            Step::Message => "message",
        }
    }

    pub fn parse(word: &str) -> Option<Step> {
        [Step::Login, Step::Post, Step::Feed, Step::Message]
            .into_iter()
            .find(|step| step.name() == word)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub steps: Vec<Step>,
}

impl Scenario {
    pub fn parse(text: &str) -> Result<Scenario, String> {
        let steps = text
            .split(',')
            .map(str::trim)
            .filter(|word| !word.is_empty())
            .map(|word| {
                Step::parse(word)
                    .ok_or_else(|| format!("шага «{word}» нет: login, post, feed, message"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if steps.is_empty() {
            return Err(String::from("сценарий пуст"));
        }
        Ok(Scenario { steps })
    }

    pub fn mixed() -> Scenario {
        Scenario {
            steps: vec![Step::Post, Step::Feed, Step::Message, Step::Feed],
        }
    }
}

pub struct User {
    pub name: String,
    pub id: String,
    pub peer: String,
    gateway: String,
    ws: Ws,
    next: u64,
    serial: u64,
}

async fn limited<T>(
    work: impl std::future::Future<Output = std::io::Result<T>>,
) -> Result<T, String> {
    match timeout(STEP_LIMIT, work).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(error.to_string()),
        Err(..) => Err(format!("дольше {STEP_LIMIT:?}")),
    }
}

impl User {
    pub async fn sign_up(gateway: &str) -> Result<User, String> {
        let name = unique_name("load");
        let body = format!(r#"{{"username": "{name}", "password": "{PASSWORD}"}}"#);
        let mut web = Web::new(gateway);
        let registered = limited(web.request("POST", "/auth/register", Some(&body), None)).await?;
        if registered.status != 201 {
            return Err(format!("регистрация {name}: HTTP {}", registered.status));
        }
        let view: UserView = decode(&registered.body).map_err(|error| error.to_string())?;
        let logged = limited(web.request("POST", "/auth/login", Some(&body), None)).await?;
        let login: Login = decode(&logged.body).map_err(|error| error.to_string())?;
        let ws = limited(Ws::connect(gateway, &format!("/ws?token={}", login.token))).await?;
        Ok(User {
            name,
            id: view.id.to_string(),
            peer: view.id.to_string(),
            gateway: gateway.to_string(),
            ws,
            next: 0,
            serial: 0,
        })
    }

    async fn call(&mut self, method: &str, data: &str) -> Result<(), String> {
        self.next += 1;
        let id = self.next;
        let service = method.split('.').next().unwrap_or(method);
        let line = format!(
            r#"{{"id": {id}, "service": "{service}", "method": "{method}", "data": {data}}}"#
        );
        limited(self.ws.send_text(&line)).await?;
        loop {
            let Some(raw) = limited(self.ws.recv()).await? else {
                return Err(String::from("шлюз закрыл WebSocket"));
            };
            let Ok(value) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            return match value.get("error") {
                Some(error) => Err(error.to_string()),
                None => Ok(()),
            };
        }
    }

    pub async fn step(&mut self, step: Step) -> Sample {
        let started = Instant::now();
        self.serial += 1;
        let done = match step {
            Step::Login => {
                let body = format!(
                    r#"{{"username": "{}", "password": "{PASSWORD}"}}"#,
                    self.name
                );
                let mut web = Web::new(&self.gateway);
                match limited(web.request("POST", "/auth/login", Some(&body), None)).await {
                    Ok(reply) if reply.status == 200 => Ok(()),
                    Ok(reply) => Err(format!("HTTP {}", reply.status)),
                    Err(error) => Err(error),
                }
            }
            Step::Post => {
                let data = format!(r#"{{"text": "нагрузка {} от {}"}}"#, self.serial, self.name);
                self.call("posts.create", &data).await
            }
            Step::Feed => self.call("posts.feed", "{}").await,
            Step::Message => {
                let data = format!(
                    r#"{{"peer": "{}", "text": "привет {}"}}"#,
                    self.peer, self.serial
                );
                self.call("chat.send", &data).await
            }
        };
        Sample {
            step: step.name(),
            took: started.elapsed(),
            ok: done.is_ok(),
        }
    }
}

pub async fn stack_target(gateway: &str, users: usize, scenario: Scenario) -> Result<Hit, String> {
    let mut prepared = Vec::new();
    for _ in 0..users.max(2) {
        prepared.push(User::sign_up(gateway).await?);
    }
    let ids: Vec<String> = prepared.iter().map(|user| user.id.clone()).collect();
    for (at, user) in prepared.iter_mut().enumerate() {
        user.peer = ids[(at + 1) % ids.len()].clone();
    }
    let pool: Arc<Vec<Mutex<User>>> = Arc::new(prepared.into_iter().map(Mutex::new).collect());
    let steps = Arc::new(scenario.steps);
    Ok(Arc::new(move |worker| {
        let pool = Arc::clone(&pool);
        let steps = Arc::clone(&steps);
        Box::pin(async move {
            let mut user = pool[worker % pool.len()].lock().await;
            let mut samples = Vec::with_capacity(steps.len());
            for step in steps.iter() {
                samples.push(user.step(*step).await);
            }
            samples
        })
    }))
}
