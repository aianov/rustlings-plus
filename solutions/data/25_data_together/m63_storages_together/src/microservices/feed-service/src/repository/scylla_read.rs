use std::collections::HashSet;

use contracts::{PostId, UserId};
use store_engine::StoreError;

use super::scylla::{Details, ScyllaFeed, big, feed_key, small};
use crate::common::{Cursor, FeedItem};

impl ScyllaFeed {
    pub(super) async fn details(&self, post: i64) -> Result<Option<Details>, StoreError> {
        self.session
            .execute(&self.st.details, (post,))
            .await?
            .maybe_first_row_typed::<Details>()
            .map_err(|error| StoreError::Other(error.to_string()))
    }

    pub(super) async fn muted(&self, viewer: UserId) -> Result<HashSet<u64>, StoreError> {
        let rows = self
            .session
            .execute(&self.st.mutes, (big(viewer.0),))
            .await?
            .rows_typed_or_empty::<(i64,)>();
        let mut muted = HashSet::new();
        for row in rows {
            let (author,) = row.map_err(|error| StoreError::Other(error.to_string()))?;
            muted.insert(small(author));
        }
        Ok(muted)
    }

    pub(super) async fn ids(
        &self,
        viewer: UserId,
        before: Option<Cursor>,
        take: usize,
    ) -> Result<Vec<(u64, u64)>, StoreError> {
        if before.is_none()
            && let Some(cache) = &self.cache
            && let Ok(cached) = cache
                .lrange(&feed_key(viewer), 0, big(take as u64) - 1)
                .await
            && cached.len() >= take
        {
            return Ok(cached
                .iter()
                .filter_map(|entry| {
                    let (at, post) = entry.split_once(':')?;
                    Some((at.parse().ok()?, post.parse().ok()?))
                })
                .collect());
        }
        let (at, post) = before.map_or((i64::MAX, i64::MAX), |cursor| {
            (big(cursor.at), big(cursor.post.0))
        });
        let rows = self
            .session
            .execute(
                &self.st.page,
                (
                    big(viewer.0),
                    at,
                    post,
                    i32::try_from(take).unwrap_or(i32::MAX),
                ),
            )
            .await?
            .rows_typed_or_empty::<(i64, i64)>();
        let mut found = Vec::new();
        for row in rows {
            let (at, post) = row.map_err(|error| StoreError::Other(error.to_string()))?;
            found.push((small(at), small(post)));
        }
        Ok(found)
    }

    pub(super) async fn page_after(
        &self,
        viewer: UserId,
        before: Option<Cursor>,
        take: usize,
    ) -> Result<(Vec<FeedItem>, Option<Cursor>), StoreError> {
        let muted = self.muted(viewer).await?;
        let ids = self.ids(viewer, before, take).await?;
        let last = ids.last().map(|(at, post)| Cursor {
            at: *at,
            post: PostId(*post),
        });
        let mut items = Vec::with_capacity(ids.len());
        for (_, post) in &ids {
            let Some((post, author, author_name, body, likes, tags, deleted, at)) =
                self.details(big(*post)).await?
            else {
                continue;
            };
            if deleted || muted.contains(&small(author)) {
                continue;
            }
            items.push(FeedItem {
                post: PostId(small(post)),
                author: UserId(small(author)),
                author_name,
                text: body,
                likes: usize::try_from(likes).unwrap_or(0),
                tags: tags.unwrap_or_default(),
                links: Vec::new(),
                at: small(at),
            });
        }
        let next = if ids.len() == take { last } else { None };
        Ok((items, next))
    }
}
