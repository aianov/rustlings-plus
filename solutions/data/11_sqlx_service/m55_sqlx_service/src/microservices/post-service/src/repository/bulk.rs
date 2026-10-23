use std::collections::HashMap;

use contracts::{PostId, UserId};
use db_engine::DbError;
use sqlx::{Acquire, PgConnection, Postgres};

use super::pg::{PgPosts, PostRow, small};
use crate::common::{Link, Post};

const WRITE_TAGS: &str = "INSERT INTO post_tags (post, position, tag) SELECT $1, t.position, t.tag FROM unnest($2::int[], $3::text[]) AS t(position, tag)";
const WRITE_LINKS: &str = "INSERT INTO post_links (post, position, url, title) SELECT $1, l.position, l.url, l.title FROM unnest($2::int[], $3::text[], $4::text[]) AS l(position, url, title)";
const TAGS_OF: &str =
    "SELECT post, tag FROM post_tags WHERE post = ANY($1) ORDER BY post, position";
const LINKS_OF: &str =
    "SELECT post, url, title FROM post_links WHERE post = ANY($1) ORDER BY post, position";

fn positions(count: usize) -> Vec<i32> {
    (0..count)
        .map(|position| i32::try_from(position).unwrap_or(i32::MAX))
        .collect()
}

pub async fn tags_with<'a, A>(conn: A, post: i64, tags: &[String]) -> Result<(), sqlx::Error>
where
    A: Acquire<'a, Database = Postgres>,
{
    let mut conn = conn.acquire().await?;
    write_tags(&mut conn, post, tags).await
}

pub async fn write_tags(
    conn: &mut PgConnection,
    post: i64,
    tags: &[String],
) -> Result<(), sqlx::Error> {
    if tags.is_empty() {
        return Ok(());
    }
    sqlx::query(WRITE_TAGS)
        .bind(post)
        .bind(positions(tags.len()))
        .bind(tags)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn write_links(
    conn: &mut PgConnection,
    post: i64,
    links: &[Link],
) -> Result<(), sqlx::Error> {
    if links.is_empty() {
        return Ok(());
    }
    let urls: Vec<String> = links.iter().map(|link| link.url.clone()).collect();
    let titles: Vec<Option<String>> = links.iter().map(|link| link.title.clone()).collect();
    sqlx::query(WRITE_LINKS)
        .bind(post)
        .bind(positions(links.len()))
        .bind(urls)
        .bind(titles)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

impl PgPosts {
    pub(super) async fn assemble(&self, rows: Vec<PostRow>) -> Result<Vec<Post>, DbError> {
        let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
        let tags: Vec<(i64, String)> = sqlx::query_as(TAGS_OF)
            .bind(&ids)
            .fetch_all(&self.pool)
            .await?;
        let links: Vec<(i64, String, Option<String>)> = sqlx::query_as(LINKS_OF)
            .bind(&ids)
            .fetch_all(&self.pool)
            .await?;
        let mut tags_of: HashMap<i64, Vec<String>> = HashMap::new();
        for (post, tag) in tags {
            tags_of.entry(post).or_default().push(tag);
        }
        let mut links_of: HashMap<i64, Vec<Link>> = HashMap::new();
        for (post, url, title) in links {
            links_of.entry(post).or_default().push(Link { url, title });
        }
        Ok(rows
            .into_iter()
            .map(|row| Post {
                id: PostId(small(row.id)),
                author: UserId(small(row.author)),
                author_name: row.author_name,
                text: row.text,
                tags: tags_of.remove(&row.id).unwrap_or_default(),
                links: links_of.remove(&row.id).unwrap_or_default(),
                likes: usize::try_from(row.likes).unwrap_or(0),
                at: small(row.at),
                deleted: row.deleted,
            })
            .collect())
    }
}
