use std::collections::VecDeque;
use std::net::SocketAddr;

use contracts::{ErrorBody, FeedPush, Outcome, Push, Response, decode};
use serde::de::DeserializeOwned;
use social_cli::Ws;
use test_support::EVENTUALLY;
use tokio::time::timeout;

#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Push(Push),
    Feed(FeedPush),
    Reply(String),
}

pub struct Client {
    ws: Ws,
    next: u64,
    pushes: VecDeque<Push>,
    feeds: VecDeque<FeedPush>,
}

fn id_of(raw: &str) -> Option<u64> {
    let rest = raw.strip_prefix(r#"{"id":"#)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

impl Client {
    pub async fn connect(addr: SocketAddr, path: &str) -> Client {
        let ws = timeout(EVENTUALLY, Ws::connect(&addr.to_string(), path))
            .await
            .unwrap_or_else(|_| panic!("WebSocket {path}: шлюз молчит"))
            .unwrap_or_else(|error| panic!("шлюз открывает WebSocket на {path}: {error}"));
        Client {
            ws,
            next: 0,
            pushes: VecDeque::new(),
            feeds: VecDeque::new(),
        }
    }

    async fn line(&mut self) -> Option<Line> {
        let raw = timeout(EVENTUALLY, self.ws.recv())
            .await
            .unwrap_or_else(|_| panic!("от шлюза ничего не пришло за {EVENTUALLY:?}"))
            .ok()
            .flatten()?;
        if let Ok(push) = decode::<Push>(&raw) {
            return Some(Line::Push(push));
        }
        match decode::<FeedPush>(&raw) {
            Ok(feed) => Some(Line::Feed(feed)),
            Err(_) => Some(Line::Reply(raw)),
        }
    }

    pub async fn send(&mut self, method: &str, data: &str) -> u64 {
        self.next += 1;
        let service = method.split('.').next().unwrap_or(method);
        let line = format!(
            r#"{{"id": {}, "service": "{service}", "method": "{method}", "data": {data}}}"#,
            self.next
        );
        self.ws
            .send_text(&line)
            .await
            .expect("текстовый фрейм уходит в шлюз");
        self.next
    }

    pub async fn call(&mut self, method: &str, data: &str) -> String {
        let id = self.send(method, data).await;
        loop {
            match self.line().await {
                Some(Line::Push(push)) => self.pushes.push_back(push),
                Some(Line::Feed(feed)) => self.feeds.push_back(feed),
                Some(Line::Reply(raw)) if id_of(&raw) == Some(id) => return raw,
                Some(Line::Reply(raw)) => panic!("ждали ответ на запрос {id}, а пришло {raw}"),
                None => panic!("шлюз закрыл соединение, не ответив на запрос {id}"),
            }
        }
    }

    pub async fn answer<T: DeserializeOwned>(
        &mut self,
        method: &str,
        data: &str,
    ) -> Result<T, ErrorBody> {
        let raw = self.call(method, data).await;
        match decode::<Response<T>>(&raw) {
            Ok(Response {
                outcome: Outcome::Ok(value),
                ..
            }) => Ok(value),
            Ok(Response {
                outcome: Outcome::Error(body),
                ..
            }) => Err(body),
            Err(error) => panic!("ответ на {method} не разобрался ({error}): {raw}"),
        }
    }

    pub async fn ok<T: DeserializeOwned>(&mut self, method: &str, data: &str) -> T {
        self.answer(method, data)
            .await
            .unwrap_or_else(|body| panic!("{method} {data}: ждали успех, а пришло {body:?}"))
    }

    pub async fn push(&mut self) -> Push {
        if let Some(push) = self.pushes.pop_front() {
            return push;
        }
        loop {
            match self.line().await {
                Some(Line::Push(push)) => return push,
                Some(Line::Feed(feed)) => self.feeds.push_back(feed),
                Some(Line::Reply(raw)) => panic!("ждали push, а пришёл ответ {raw}"),
                None => panic!("шлюз закрыл соединение, а push так и не пришёл"),
            }
        }
    }

    pub async fn push_where(&mut self, wanted: impl Fn(&Push) -> bool) -> Push {
        let mut skipped = Vec::new();
        let found = loop {
            let push = self.push().await;
            if wanted(&push) {
                break push;
            }
            skipped.push(push);
        };
        for push in skipped.into_iter().rev() {
            self.pushes.push_front(push);
        }
        found
    }

    pub async fn feed(&mut self) -> FeedPush {
        if let Some(feed) = self.feeds.pop_front() {
            return feed;
        }
        loop {
            match self.line().await {
                Some(Line::Feed(feed)) => return feed,
                Some(Line::Push(push)) => self.pushes.push_back(push),
                Some(Line::Reply(raw)) => panic!("ждали событие ленты, а пришёл ответ {raw}"),
                None => panic!("шлюз закрыл соединение, а событие ленты так и не пришло"),
            }
        }
    }
}
