use std::time::Duration;

use serde_json::{Value, json};
use tokio::time::sleep;

use crate::router::{ApiError, Ctx, Router};

const MAX_TEXT: usize = 262_144;
const MAX_SLEEP_MS: u64 = 5000;

fn room_of(data: &Value) -> Result<String, ApiError> {
    match data.get("room").and_then(Value::as_str) {
        Some(room) if !room.is_empty() && room.len() <= 64 => Ok(room.to_string()),
        _ => Err(ApiError::new("bad_request", "room должен быть строкой от 1 до 64 байт")),
    }
}

pub fn build() -> Router {
    Router::new()
        .route("chat.join", |ctx: Ctx, data: Value| async move {
            let room = room_of(&data)?;
            let members = ctx.hub.join(&ctx.user, &room);
            Ok(json!({ "room": room, "members": members }))
        })
        .route("chat.leave", |ctx: Ctx, data: Value| async move {
            let room = room_of(&data)?;
            Ok(json!({ "left": ctx.hub.leave(&ctx.user, &room) }))
        })
        .route("chat.who", |ctx: Ctx, data: Value| async move {
            let room = room_of(&data)?;
            Ok(json!({ "room": room, "members": ctx.hub.members(&room) }))
        })
        .route("chat.say", |ctx: Ctx, data: Value| async move {
            let room = room_of(&data)?;
            let Some(text) = data.get("text").and_then(Value::as_str) else {
                return Err(ApiError::new("bad_request", "text должен быть строкой"));
            };
            if text.is_empty() || text.len() > MAX_TEXT {
                return Err(ApiError::new("too_long", format!("text от 1 до {MAX_TEXT} байт")));
            }
            match ctx.hub.say(&room, &ctx.user, ctx.conn, text) {
                Some(delivered) => Ok(json!({ "delivered": delivered })),
                None => Err(ApiError::new("not_member", "сначала войди в комнату: chat.join")),
            }
        })
        .route("sys.sleep", |_ctx: Ctx, data: Value| async move {
            let ms = data.get("ms").and_then(Value::as_u64).filter(|ms| *ms <= MAX_SLEEP_MS);
            let Some(ms) = ms else {
                return Err(ApiError::new("bad_request", format!("ms от 0 до {MAX_SLEEP_MS}")));
            };
            sleep(Duration::from_millis(ms)).await;
            Ok(json!({ "slept": ms }))
        })
        .route("sys.whoami", |ctx: Ctx, _data: Value| async move { Ok(json!({ "user": ctx.user })) })
}
