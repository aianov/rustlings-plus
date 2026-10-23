use std::sync::Arc;
use std::time::Duration;

use service_runtime::CancellationToken;

use super::{MAX_TRENDS, PostError, PostService};
use crate::common::Trend;
use crate::repository::{FullText, Hit};

pub const MAX_FIND: u32 = 50;

pub const MAX_QUERY: usize = 200;

pub const TRENDS_REFRESH: Duration = Duration::from_secs(10);

impl PostService {
    pub fn with_fulltext(self, fulltext: Arc<dyn FullText>) -> PostService {
        PostService {
            fulltext: Some(fulltext),
            ..self
        }
    }

    pub async fn find(&self, query: &str, limit: u32) -> Result<Vec<Hit>, PostError> {
        let query = query.trim();
        if query.is_empty() || query.chars().count() > MAX_QUERY {
            return Err(PostError::BadSearch(format!(
                "запрос поиска это 1..={MAX_QUERY} символов"
            )));
        }
        let fulltext = self.fulltext.clone().ok_or(PostError::NoThreads)?;
        let limit = match limit {
            0 => 10,
            limit => limit.min(MAX_FIND),
        };
        Ok(fulltext.find(query.to_string(), limit).await?)
    }

    pub(super) async fn trends_from_view(&self, limit: usize) -> Option<Vec<Trend>> {
        let fulltext = self.fulltext.as_ref()?;
        let limit = match limit {
            0 => MAX_TRENDS,
            limit => limit.min(MAX_TRENDS),
        };
        let rows = fulltext
            .trends(u32::try_from(limit).unwrap_or(u32::MAX))
            .await;
        Some(
            rows.into_iter()
                .map(|(post, likes)| Trend { post, likes })
                .collect(),
        )
    }
}

pub async fn refresh_trends(
    fulltext: Arc<dyn FullText>,
    every: Duration,
    closing: CancellationToken,
) {
    loop {
        if let Err(error) = fulltext.refresh_trends().await {
            tracing::warn!("post-service: тренды не пересчитаны: {error}");
        }
        tokio::select! {
            () = closing.cancelled() => return,
            () = tokio::time::sleep(every) => {}
        }
    }
}
