use crate::rpc::{Service, request_json, respond, sync};
use async_nats::Client;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

pub async fn start(client: &Client, p: &str) -> Service {
    let (mut service, stop) = Service::new();
    let followers: Arc<Mutex<HashMap<u64, BTreeSet<u64>>>> = Arc::default();
    let follow = {
        let (followers, client, p) = (followers.clone(), client.clone(), p.to_string());
        move |req: Value| {
            let (followers, client, p) = (followers.clone(), client.clone(), p.clone());
            async move {
                let who = req["who"].as_u64().unwrap_or(0);
                let found = request_json(&client, &format!("{p}.auth.lookup"), &json!({"name": req["whom"]})).await;
                match found {
                    Ok(v) if v["id"].is_u64() => {
                        followers.lock().unwrap().entry(v["id"].as_u64().unwrap()).or_default().insert(who);
                        json!({"ok": true})
                    }
                    _ => json!({"err": "no such user"}),
                }
            }
        }
    };
    let list = {
        let followers = followers.clone();
        move |req: Value| {
            let followers = followers.clone();
            async move {
                let ids: Vec<u64> = followers.lock().unwrap().get(&req["id"].as_u64().unwrap_or(0)).map(|s| s.iter().copied().collect()).unwrap_or_default();
                json!({"ids": ids})
            }
        }
    };
    service.tasks.push(respond(client, format!("{p}.users.follow"), stop.clone(), &service.live, follow).await);
    service.tasks.push(respond(client, format!("{p}.users.followers"), stop, &service.live, list).await);
    sync(client).await;
    service
}
