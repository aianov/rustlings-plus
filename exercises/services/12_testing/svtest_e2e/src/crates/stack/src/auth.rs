use crate::rpc::{Service, respond, sync};
use async_nats::Client;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Accounts {
    by_name: HashMap<String, (u64, String)>,
    tokens: HashMap<String, (u64, String)>,
    next: u64,
}

pub async fn start(client: &Client, p: &str) -> Service {
    let (mut service, stop) = Service::new();
    let state = Arc::new(Mutex::new(Accounts::default()));
    let register = {
        let state = state.clone();
        move |req: Value| {
            let state = state.clone();
            async move {
                let (name, password) = (req["name"].as_str().unwrap_or("").to_string(), req["password"].as_str().unwrap_or("").to_string());
                let mut accounts = state.lock().unwrap();
                if name.is_empty() || accounts.by_name.contains_key(&name) {
                    return json!({"err": "exists"});
                }
                accounts.next += 1;
                let id = accounts.next;
                accounts.by_name.insert(name, (id, password));
                json!({"id": id})
            }
        }
    };
    let login = {
        let state = state.clone();
        move |req: Value| {
            let state = state.clone();
            async move {
                let (name, password) = (req["name"].as_str().unwrap_or(""), req["password"].as_str().unwrap_or(""));
                let mut accounts = state.lock().unwrap();
                let Some((id, stored)) = accounts.by_name.get(name).cloned() else { return json!({"err": "bad credentials"}) };
                if stored != password {
                    return json!({"err": "bad credentials"});
                }
                let token = format!("tok-{id}-{}", accounts.tokens.len());
                accounts.tokens.insert(token.clone(), (id, name.to_string()));
                json!({"token": token})
            }
        }
    };
    let verify = {
        let state = state.clone();
        move |req: Value| {
            let state = state.clone();
            async move {
                match state.lock().unwrap().tokens.get(req["token"].as_str().unwrap_or("")) {
                    Some((id, name)) => json!({"id": id, "name": name}),
                    None => json!({"err": "unauthorized"}),
                }
            }
        }
    };
    let lookup = {
        let state = state.clone();
        move |req: Value| {
            let state = state.clone();
            async move {
                match state.lock().unwrap().by_name.get(req["name"].as_str().unwrap_or("")) {
                    Some((id, _)) => json!({"id": id}),
                    None => json!({"err": "no such user"}),
                }
            }
        }
    };
    service.tasks.push(respond(client, format!("{p}.auth.register"), stop.clone(), &service.live, register).await);
    service.tasks.push(respond(client, format!("{p}.auth.login"), stop.clone(), &service.live, login).await);
    service.tasks.push(respond(client, format!("{p}.auth.verify"), stop.clone(), &service.live, verify).await);
    service.tasks.push(respond(client, format!("{p}.auth.lookup"), stop, &service.live, lookup).await);
    sync(client).await;
    service
}
