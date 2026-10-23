use crate::client::Client;
use crate::codec::RespError;
use crate::command::cmd;

pub const KEY_ROOT: &str = "social";

pub fn key(service: &str, kind: &str, id: impl std::fmt::Display) -> String {
    format!("{KEY_ROOT}:{service}:{kind}:{id}")
}

pub async fn scan_keys(
    client: &Client,
    pattern: &str,
    batch: usize,
) -> Result<Vec<String>, RespError> {
    let mut cursor = String::from("0");
    let mut found = Vec::new();
    loop {
        let reply = client
            .query(
                cmd("SCAN")
                    .arg(&cursor)
                    .arg("MATCH")
                    .arg(pattern)
                    .arg("COUNT")
                    .arg(batch),
            )
            .await?;
        let mut parts = reply.items().into_iter();
        let next = parts
            .next()
            .and_then(|value| value.as_text())
            .unwrap_or_default();
        let keys = parts.next().map(|value| value.items()).unwrap_or_default();
        found.extend(keys.iter().filter_map(|key| key.as_text()));
        if next == "0" || next.is_empty() {
            return Ok(found);
        }
        cursor = next;
    }
}
