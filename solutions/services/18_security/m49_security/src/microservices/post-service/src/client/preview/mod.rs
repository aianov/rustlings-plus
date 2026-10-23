use std::time::Duration;

use http_client::{AddrGuard, Client, Config};
use tokio::time::timeout;

pub const PREVIEW_TIMEOUT: Duration = Duration::from_secs(2);

pub const PREVIEW_MAX_BODY: usize = 64 * 1024;

pub const PREVIEW_REDIRECTS: usize = 3;

pub const TITLE_MAX: usize = 200;

pub struct Previewer {
    client: Client,
}

fn entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

pub fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    let raw = entities(&html[start..end]);
    let words: Vec<&str> = raw.split_whitespace().collect();
    let title: String = words
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(TITLE_MAX)
        .collect();
    (!title.is_empty()).then_some(title)
}

impl Previewer {
    pub fn new() -> Previewer {
        Previewer::with_guard(AddrGuard::public_only())
    }

    pub fn with_guard(guard: AddrGuard) -> Previewer {
        let config = Config {
            connect_timeout: Duration::from_secs(1),
            total_timeout: PREVIEW_TIMEOUT,
            max_redirects: PREVIEW_REDIRECTS,
            max_body: PREVIEW_MAX_BODY,
            max_idle_per_host: 0,
            guard: Some(guard),
            ..Config::default()
        };
        Previewer {
            client: Client::new(config),
        }
    }

    pub async fn title(&self, url: &str) -> Option<String> {
        let lower = url.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return None;
        }
        let response = timeout(PREVIEW_TIMEOUT, self.client.get(url))
            .await
            .ok()?
            .ok()?;
        if !(200..300).contains(&response.status) {
            return None;
        }
        let html = response
            .header("content-type")
            .is_none_or(|kind| kind.to_ascii_lowercase().starts_with("text/html"));
        if !html {
            return None;
        }
        extract_title(&response.text())
    }
}

impl Default for Previewer {
    fn default() -> Previewer {
        Previewer::new()
    }
}
