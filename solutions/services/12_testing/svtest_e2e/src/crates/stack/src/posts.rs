use crate::rpc::{Service, request_json, respond, sync};
use async_nats::Client;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub async fn start(client: &Client, p: &str) -> Service {
    let (mut service, stop) = Service::new();
    let counter = Arc::new(AtomicU64::new(0));
    let create = {
        let (client, p) = (client.clone(), p.to_string());
        move |req: Value| {
            let (client, p, counter) = (client.clone(), p.clone(), counter.clone());
            async move {
                let who = request_json(&client, &format!("{p}.auth.verify"), &json!({"token": req["token"]})).await;
                let Ok(who) = who else { return json!({"err": "unavailable"}) };
                if who["id"].is_null() {
                    return json!({"err": "unauthorized"});
                }
                let post_id = counter.fetch_add(1, Ordering::SeqCst) + 1;
                let event = json!({"post_id": post_id, "author_id": who["id"], "author_name": who["name"], "text": req["text"]});
                if client.publish(format!("{p}.event.post.created"), event.to_string().into()).await.is_err() {
                    return json!({"err": "unavailable"});
                }
                json!({"ok": true, "post_id": post_id})
            }
        }
    };
    service.tasks.push(respond(client, format!("{p}.posts.create"), stop, &service.live, create).await);
    sync(client).await;
    service
}
