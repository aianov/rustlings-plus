use std::sync::atomic::{AtomicU64, Ordering};

pub const PASSWORD: &str = "correct-horse";

static SERIAL: AtomicU64 = AtomicU64::new(0);

pub fn unique_name(prefix: &str) -> String {
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id() % 10_000;
    let base: String = prefix
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect::<String>()
        .to_ascii_lowercase();
    let base = if base.is_empty() {
        String::from("user")
    } else {
        base
    };
    format!("{base}{pid}x{serial}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewUser {
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
}

pub fn user() -> NewUser {
    NewUser {
        username: unique_name("user"),
        password: PASSWORD.to_string(),
        display_name: None,
        bio: None,
    }
}

fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl NewUser {
    pub fn named(self, prefix: &str) -> NewUser {
        NewUser {
            username: unique_name(prefix),
            ..self
        }
    }

    pub fn exactly(self, username: &str) -> NewUser {
        NewUser {
            username: username.to_string(),
            ..self
        }
    }

    pub fn password(self, password: &str) -> NewUser {
        NewUser {
            password: password.to_string(),
            ..self
        }
    }

    pub fn display_name(self, name: &str) -> NewUser {
        NewUser {
            display_name: Some(name.to_string()),
            ..self
        }
    }

    pub fn bio(self, bio: &str) -> NewUser {
        NewUser {
            bio: Some(bio.to_string()),
            ..self
        }
    }

    pub fn credentials(&self) -> String {
        format!(
            "{{\"username\": {}, \"password\": {}}}",
            quoted(&self.username),
            quoted(&self.password)
        )
    }

    pub fn profile_patch(&self) -> Option<String> {
        let mut fields = Vec::new();
        if let Some(name) = &self.display_name {
            fields.push(format!("\"display_name\": {}", quoted(name)));
        }
        if let Some(bio) = &self.bio {
            fields.push(format!("\"bio\": {}", quoted(bio)));
        }
        (!fields.is_empty()).then(|| format!("{{{}}}", fields.join(", ")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPost {
    pub text: String,
}

pub fn post() -> NewPost {
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    NewPost {
        text: format!("пост номер {serial}"),
    }
}

impl NewPost {
    pub fn text(self, text: &str) -> NewPost {
        NewPost {
            text: text.to_string(),
        }
    }

    pub fn tagged(self, tag: &str) -> NewPost {
        NewPost {
            text: format!("{} #{tag}", self.text),
        }
    }

    pub fn body(&self) -> String {
        format!("{{\"text\": {}}}", quoted(&self.text))
    }
}
