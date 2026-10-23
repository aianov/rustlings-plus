use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PostEntry {
    Posted {
        id: u64,
        author: u64,
        author_name: String,
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        links: Vec<LinkEntry>,
        at: u64,
    },
    Liked {
        post: u64,
        by: u64,
        at: u64,
    },
    Deleted {
        post: u64,
    },
    Commented {
        id: u64,
        post: u64,
        author: u64,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkEntry {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}
