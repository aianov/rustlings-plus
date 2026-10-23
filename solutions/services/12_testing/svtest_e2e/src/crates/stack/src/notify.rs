use crate::rpc::{Alive, Service, request_json, respond, sync};
use async_nats::Client;
use futures::StreamExt;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub async fn start(client: &Client, p: &str) -> Service {
    let (mut service, mut stop) = Service::new();
    let counts: Arc<Mutex<HashMap<u64, u64>>> = Arc::default();
    let mut events = client.subscribe(format!("{p}.event.post.created")).await.expect("подписка на события");
    let alive = Alive::enter(&service.live);
    let worker = tokio::spawn({
        let (client, p, counts) = (client.clone(), p.to_string(), counts.clone());
        async move {
            let _alive = alive;
            loop {
                tokio::select! {
                    _ = stop.changed() => return,
                    message = events.next() => {
                        let Some(message) = message else { return };
                        let Ok(event) = serde_json::from_slice::<Value>(&message.payload) else { continue };
                        let Ok(found) = request_json(&client, &format!("{p}.users.followers"), &json!({"id": event["author_id"]})).await else { continue };
                        let push = format!("{} {}", event["author_name"].as_str().unwrap_or("?"), event["text"].as_str().unwrap_or(""));
                        for id in found["ids"].as_array().cloned().unwrap_or_default() {
                            let id = id.as_u64().unwrap_or(0);
                            *counts.lock().unwrap().entry(id).or_default() += 1;
                            let _ = client.publish(format!("{p}.push.{id}"), push.clone().into()).await;
                        }
                    }
                }
            }
        }
    });
    service.tasks.push(worker);
    let count = {
        let counts = counts.clone();
        move |req: Value| {
            let counts = counts.clone();
            async move { json!({"n": counts.lock().unwrap().get(&req["id"].as_u64().unwrap_or(0)).copied().unwrap_or(0)}) }
        }
    };
    let stop = service.stop.subscribe();
    service.tasks.push(respond(client, format!("{p}.notify.count"), stop, &service.live, count).await);
    sync(client).await;
    service
}
