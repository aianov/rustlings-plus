use std::sync::Arc;

use contracts::{PostId, UserId};

use super::PostService;
use crate::common::Post;

pub const MAX_TAGS_TOP: usize = 20;

pub type BoxHeat<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

pub trait Heat: Send + Sync {
    fn view(&self, post: PostId, viewer: UserId) -> BoxHeat<'_, u64>;
    fn views(&self, posts: Vec<PostId>) -> BoxHeat<'_, Vec<u64>>;
    fn tagged(&self, tags: Vec<String>, at: u64) -> BoxHeat<'_, ()>;
    fn top_tags(&self, limit: usize, now: u64) -> BoxHeat<'_, Vec<(String, u64)>>;
}

impl PostService {
    pub fn with_heat(self, heat: Arc<dyn Heat>) -> PostService {
        PostService {
            heat: Some(heat),
            ..self
        }
    }

    pub async fn viewed(&self, post: PostId, viewer: Option<UserId>) -> u64 {
        let Some(heat) = &self.heat else {
            return 0;
        };
        match viewer {
            Some(viewer) => heat.view(post, viewer).await,
            None => heat.views(vec![post]).await.first().copied().unwrap_or(0),
        }
    }

    pub async fn views_of(&self, posts: &[PostId]) -> Vec<u64> {
        match &self.heat {
            Some(heat) => heat.views(posts.to_vec()).await,
            None => vec![0; posts.len()],
        }
    }

    pub async fn tagged(&self, post: &Post) {
        if let Some(heat) = &self.heat
            && !post.tags.is_empty()
        {
            heat.tagged(post.tags.clone(), post.at).await;
        }
    }

    pub async fn top_tags(&self, limit: usize) -> Vec<(String, u64)> {
        let limit = match limit {
            0 => 10,
            limit => limit.min(MAX_TAGS_TOP),
        };
        match &self.heat {
            Some(heat) => heat.top_tags(limit, crate::common::now_micros()).await,
            None => Vec::new(),
        }
    }
}
