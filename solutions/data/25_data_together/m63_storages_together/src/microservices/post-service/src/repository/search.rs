use contracts::UserId;
use db_engine::{DbError, contains_pattern};
use service_runtime::StorageError;
use sqlx::{Postgres, QueryBuilder};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use super::BoxFuture;
use super::pg::{PgPosts, PostRow, big, storage};
use crate::common::Post;

const EXPORT: &str = "SELECT id, author, author_name, body AS text, likes, at, deleted FROM posts WHERE author = $1 AND NOT deleted ORDER BY id";

pub const AUTHOR_PAGE: &str = "SELECT id, author, author_name, body AS text, likes, at, deleted FROM posts WHERE author = $1 AND NOT deleted AND (at, id) < ($2, $3) ORDER BY at DESC, id DESC LIMIT $4";

pub const EXPORT_BATCH: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    New,
    Old,
    Likes,
}

impl Sort {
    pub fn parse(raw: &str) -> Option<Sort> {
        match raw {
            "" | "new" => Some(Sort::New),
            "old" => Some(Sort::Old),
            "likes" => Some(Sort::Likes),
            _ => None,
        }
    }

    fn push_order(self, query: &mut QueryBuilder<'_, Postgres>) {
        query.push(match self {
            Sort::New => " ORDER BY p.id DESC",
            Sort::Old => " ORDER BY p.id ASC",
            Sort::Likes => " ORDER BY p.likes DESC, p.id DESC",
        });
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Search {
    pub author: Option<UserId>,
    pub tag: Option<String>,
    pub text: Option<String>,
    pub since: Option<u64>,
    pub sort: Sort,
    pub limit: u32,
}

pub trait SearchQueries: Send + Sync {
    fn search(&self, search: Search) -> BoxFuture<'_, Result<Vec<Post>, StorageError>>;
    fn export(
        &self,
        author: UserId,
        sink: mpsc::Sender<Post>,
    ) -> BoxFuture<'_, Result<usize, StorageError>>;
    fn author_page(
        &self,
        author: UserId,
        before: Option<(u64, u64)>,
        limit: u32,
    ) -> BoxFuture<'_, Result<Vec<Post>, StorageError>>;
}

fn built(search: &Search) -> QueryBuilder<'_, Postgres> {
    let mut query = QueryBuilder::new(
        "SELECT p.id, p.author, p.author_name, p.body AS text, p.likes, p.at, p.deleted FROM posts p WHERE NOT p.deleted",
    );
    if let Some(author) = search.author {
        query.push(" AND p.author = ").push_bind(big(author.0));
    }
    if let Some(tag) = &search.tag {
        query
            .push(" AND EXISTS (SELECT 1 FROM post_tags t WHERE t.post = p.id AND t.tag = ")
            .push_bind(tag.clone())
            .push(")");
    }
    if let Some(text) = &search.text {
        query
            .push(" AND p.body ILIKE ")
            .push_bind(contains_pattern(text))
            .push(" ESCAPE '\\'");
    }
    if let Some(since) = search.since {
        query.push(" AND p.at >= ").push_bind(big(since));
    }
    search.sort.push_order(&mut query);
    query.push(" LIMIT ").push_bind(i64::from(search.limit));
    query
}

impl SearchQueries for PgPosts {
    fn author_page(
        &self,
        author: UserId,
        before: Option<(u64, u64)>,
        limit: u32,
    ) -> BoxFuture<'_, Result<Vec<Post>, StorageError>> {
        Box::pin(async move {
            let (at, id) = before.map_or((i64::MAX, i64::MAX), |(at, id)| (big(at), big(id)));
            let rows: Vec<PostRow> = sqlx::query_as(AUTHOR_PAGE)
                .bind(big(author.0))
                .bind(at)
                .bind(id)
                .bind(i64::from(limit))
                .fetch_all(&self.pool)
                .await
                .map_err(storage)?;
            self.assemble(rows).await.map_err(storage)
        })
    }

    fn search(&self, search: Search) -> BoxFuture<'_, Result<Vec<Post>, StorageError>> {
        Box::pin(async move {
            let mut query = built(&search);
            let rows: Vec<PostRow> = query
                .build_query_as()
                .fetch_all(&self.pool)
                .await
                .map_err(storage)?;
            self.assemble(rows).await.map_err(storage)
        })
    }

    fn export(
        &self,
        author: UserId,
        sink: mpsc::Sender<Post>,
    ) -> BoxFuture<'_, Result<usize, StorageError>> {
        Box::pin(async move {
            let mut rows = sqlx::query_as::<_, PostRow>(EXPORT)
                .bind(big(author.0))
                .fetch(&self.pool);
            let mut batch = Vec::with_capacity(EXPORT_BATCH);
            let mut sent = 0;
            loop {
                let next = rows.next().await;
                let done = next.is_none();
                if let Some(row) = next {
                    batch.push(row.map_err(|error| storage(DbError::from(error)))?);
                }
                if batch.len() == EXPORT_BATCH || (done && !batch.is_empty()) {
                    let posts = self.assemble(std::mem::take(&mut batch)).await;
                    for post in posts.map_err(storage)? {
                        if sink.send(post).await.is_err() {
                            return Ok(sent);
                        }
                        sent += 1;
                    }
                }
                if done {
                    return Ok(sent);
                }
            }
        })
    }
}
