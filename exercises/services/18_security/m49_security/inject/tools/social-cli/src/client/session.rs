use std::io;

use serde_json::Value;
use tokio::io::AsyncWrite;

use super::Client;
use crate::connect_after;
use crate::http::Reply;
use crate::render::text;

const REFRESH: &str = "refresh";

fn refresh_cookie(reply: &Reply) -> Option<Option<String>> {
    reply
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .find_map(|(_, value)| {
            let pair = value.split(';').next()?.trim();
            let (name, token) = pair.split_once('=')?;
            (name == REFRESH).then(|| (!token.is_empty()).then(|| token.to_string()))
        })
}

impl<W: AsyncWrite + Unpin> Client<W> {
    pub(super) fn remember_refresh(&mut self, reply: &Reply) {
        if let Some(cookie) = refresh_cookie(reply) {
            self.refresh = cookie;
        }
    }

    pub(super) async fn forget(&mut self) -> io::Result<()> {
        self.token = None;
        self.refresh = None;
        self.hang_up().await
    }

    pub(super) async fn adopt(&mut self, login: &Value) -> io::Result<bool> {
        let token = text(login, "token").unwrap_or_default().to_string();
        self.hang_up().await?;
        let after = self.last_seq;
        match connect_after(&self.dialer, &self.addr, &token, after, 1).await {
            Ok(ws) => {
                self.ws = Some(ws);
                if let Some(after) = after {
                    let line = format!("догоняю сообщения после №{after}");
                    self.say(&line).await?;
                }
            }
            Err(error) => {
                let line = format!("WebSocket не открылся: {error}");
                self.say(&line).await?;
                return Ok(false);
            }
        }
        self.token = Some(token);
        Ok(true)
    }

    pub(super) async fn renew(&mut self) -> io::Result<bool> {
        let Some(refresh) = self.refresh.clone() else {
            self.forget().await?;
            return Ok(false);
        };
        let cookie = format!("{REFRESH}={refresh}");
        let headers = [
            ("Cookie", cookie.as_str()),
            ("X-Social-Client", "social-cli"),
        ];
        let reply = self
            .web
            .send("POST", "/auth/refresh", None, &headers)
            .await?;
        self.remember_refresh(&reply);
        let login: Value = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
        if reply.status != 200 {
            self.forget().await?;
            return Ok(false);
        }
        if !self.adopt(&login).await? {
            return Ok(false);
        }
        self.say("токен обновлён по refresh").await?;
        Ok(true)
    }
}
