use std::sync::Arc;

use contracts::UserId;
use tokio::sync::mpsc;

use super::{PostError, PostService};
use crate::common::Post;
use crate::repository::{EXPORT_BATCH, Search, SearchQueries, Sort};

pub const MAX_FOUND: u32 = 50;

pub const MAX_SEARCH_TEXT: usize = 100;

pub struct SearchAsk {
    pub author: Option<UserId>,
    pub tag: Option<String>,
    pub text: Option<String>,
    pub since: Option<u64>,
    pub sort: String,
    pub limit: u32,
}

fn cursor(raw: &str) -> Result<Option<(u64, u64)>, PostError> {
    if raw.is_empty() {
        return Ok(None);
    }
    let parsed = raw
        .split_once('.')
        .and_then(|(at, id)| Some((at.parse().ok()?, id.parse().ok()?)));
    parsed
        .map(Some)
        .ok_or_else(|| PostError::BadSearch(format!("курсор «{raw}» не из прошлого ответа")))
}

impl PostService {
    pub async fn author_page(
        &self,
        author: UserId,
        raw: &str,
        limit: usize,
    ) -> Result<(Vec<Post>, String), PostError> {
        let Some(search) = self.search.clone() else {
            return Ok((self.by_author(author, limit).await, String::new()));
        };
        let before = cursor(raw)?;
        let take = u32::try_from(limit).unwrap_or(u32::MAX);
        let posts = search.author_page(author, before, take).await?;
        let next = match posts.last() {
            Some(last) if posts.len() == limit => format!("{}.{}", last.at, last.id.0),
            _ => String::new(),
        };
        Ok((posts, next))
    }

    pub fn with_search(self, search: Arc<dyn SearchQueries>) -> PostService {
        PostService {
            search: Some(search),
            ..self
        }
    }

    fn searcher(&self) -> Result<Arc<dyn SearchQueries>, PostError> {
        self.search.clone().ok_or(PostError::NoThreads)
    }

    pub async fn search(&self, ask: SearchAsk) -> Result<Vec<Post>, PostError> {
        let Some(sort) = Sort::parse(&ask.sort) else {
            return Err(PostError::BadSearch(format!(
                "сортировка «{}» не из new, old, likes",
                ask.sort
            )));
        };
        if ask
            .text
            .as_ref()
            .is_some_and(|text| text.chars().count() > MAX_SEARCH_TEXT)
        {
            return Err(PostError::BadSearch(format!(
                "текст поиска не длиннее {MAX_SEARCH_TEXT} символов"
            )));
        }
        let limit = match ask.limit {
            0 => 20,
            limit => limit.min(MAX_FOUND),
        };
        let search = Search {
            author: ask.author,
            tag: ask.tag,
            text: ask.text,
            since: ask.since,
            sort,
            limit,
        };
        Ok(self.searcher()?.search(search).await?)
    }

    pub fn export(&self, author: UserId) -> Result<mpsc::Receiver<Post>, PostError> {
        let search = self.searcher()?;
        let (tx, rx) = mpsc::channel(EXPORT_BATCH);
        tokio::spawn(async move {
            if let Err(error) = search.export(author, tx).await {
                tracing::warn!("post-service: выгрузка постов оборвалась: {error}");
            }
        });
        Ok(rx)
    }
}
