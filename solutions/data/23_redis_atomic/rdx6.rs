use course_support::redis::{Client, Error, Value};

struct Script {
    body: &'static str,
    sha: String,
}

impl Script {
    async fn load(c: &Client, body: &'static str) -> Result<Script, Error> {
        let sha = c.cmd(&["SCRIPT", "LOAD", body]).await?.text().ok_or_else(|| Error::Protocol("нет хеша".into()))?;
        Ok(Script { body, sha })
    }

    async fn run(&self, c: &Client, key: &str) -> Result<Value, Error> {
        match c.cmd(&["EVALSHA", &self.sha, "1", key]).await {
            Err(Error::Server(e)) if e.starts_with("NOSCRIPT") => c.cmd(&["EVAL", self.body, "1", key]).await,
            other => other,
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    const BUMP: &str = "return redis.call('INCR', KEYS[1])";

    #[tokio::test]
    async fn survives_script_cache_loss() {
        let s = Scope::new().await;
        let c = s.client();
        let key = s.key("bumps");
        let script = Script::load(&c, BUMP).await.expect("загрузка скрипта");
        assert_eq!(script.run(&c, &key).await.expect("по хешу").int(), Some(1));
        let before = c.round_trips();
        assert_eq!(script.run(&c, &key).await.expect("по хешу").int(), Some(2));
        assert_eq!(c.round_trips() - before, 1, "пока скрипт в кэше, вызов это один обмен по хешу");
        s.cmd(&["SCRIPT", "FLUSH"]).await.expect("сброс кэша скриптов");
        assert_eq!(script.run(&c, &key).await.expect("после сброса кэша вызов должен пережить NOSCRIPT").int(), Some(3));
        let before = c.round_trips();
        assert_eq!(script.run(&c, &key).await.expect("снова по хешу").int(), Some(4));
        assert_eq!(c.round_trips() - before, 1, "EVAL вернул скрипт в кэш, дальше снова по хешу");
    }
}
