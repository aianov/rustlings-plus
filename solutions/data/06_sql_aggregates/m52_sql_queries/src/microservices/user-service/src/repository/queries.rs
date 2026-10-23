use std::collections::BTreeSet;

use contracts::UserId;

use super::pg::{PgUsers, UserRow, id, user_of};
use super::{BoxFuture, MemoryUsers, SocialQueries};
use crate::common::User;

const FOLLOWERS: &str = "SELECT u.id, u.username, u.display_name, u.bio FROM follows f JOIN users u ON u.id = f.follower WHERE f.followee = $1 AND u.id > $2 AND NOT u.hidden AND NOT u.deleted ORDER BY u.id LIMIT $3";
const FOLLOWING: &str = "SELECT u.id, u.username, u.display_name, u.bio FROM follows f JOIN users u ON u.id = f.followee WHERE f.follower = $1 AND u.id > $2 AND NOT u.hidden AND NOT u.deleted ORDER BY u.id LIMIT $3";
const FRIENDS: &str = "SELECT u.id, u.username, u.display_name, u.bio FROM follows mine JOIN follows theirs ON theirs.follower = mine.followee AND theirs.followee = mine.follower JOIN users u ON u.id = mine.followee WHERE mine.follower = $1 AND u.id > $2 AND NOT u.hidden AND NOT u.deleted ORDER BY u.id LIMIT $3";
const COUNTERS: &str = "SELECT (SELECT count(*) FROM follows f JOIN users u ON u.id = f.follower WHERE f.followee = $1 AND NOT u.deleted) AS followers, (SELECT count(*) FROM follows f JOIN users u ON u.id = f.followee WHERE f.follower = $1 AND NOT u.deleted) AS following";

fn big(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl PgUsers {
    async fn related(&self, query: &str, user: UserId, after: u64, limit: usize) -> Vec<User> {
        let rows: Vec<UserRow> = sqlx::query_as(query)
            .bind(id(user))
            .bind(big(after))
            .bind(big(limit as u64))
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
        rows.into_iter().filter_map(user_of).collect()
    }
}

impl SocialQueries for PgUsers {
    fn followers(&self, user: UserId, after: u64, limit: usize) -> BoxFuture<'_, Vec<User>> {
        Box::pin(async move { self.related(FOLLOWERS, user, after, limit).await })
    }

    fn following(&self, user: UserId, after: u64, limit: usize) -> BoxFuture<'_, Vec<User>> {
        Box::pin(async move { self.related(FOLLOWING, user, after, limit).await })
    }

    fn friends(&self, user: UserId, after: u64, limit: usize) -> BoxFuture<'_, Vec<User>> {
        Box::pin(async move { self.related(FRIENDS, user, after, limit).await })
    }

    fn counters(&self, user: UserId) -> BoxFuture<'_, (u64, u64)> {
        Box::pin(async move {
            let row: (i64, i64) = sqlx::query_as(COUNTERS)
                .bind(id(user))
                .fetch_one(&self.pool)
                .await
                .unwrap_or((0, 0));
            (
                u64::try_from(row.0).unwrap_or(0),
                u64::try_from(row.1).unwrap_or(0),
            )
        })
    }
}

fn page(state: &super::state::State, ids: BTreeSet<UserId>, after: u64, limit: usize) -> Vec<User> {
    ids.into_iter()
        .filter(|user| user.0 > after)
        .filter_map(|user| state.user(user))
        .take(limit)
        .collect()
}

impl SocialQueries for MemoryUsers {
    fn followers(&self, user: UserId, after: u64, limit: usize) -> BoxFuture<'_, Vec<User>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let ids = state
                .followers
                .get(&user)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            page(&state, ids, after, limit)
        })
    }

    fn following(&self, user: UserId, after: u64, limit: usize) -> BoxFuture<'_, Vec<User>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let ids = state
                .following
                .get(&user)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            page(&state, ids, after, limit)
        })
    }

    fn friends(&self, user: UserId, after: u64, limit: usize) -> BoxFuture<'_, Vec<User>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let mine: BTreeSet<UserId> = state
                .following
                .get(&user)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            let theirs: BTreeSet<UserId> = state
                .followers
                .get(&user)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            page(
                &state,
                mine.intersection(&theirs).copied().collect(),
                after,
                limit,
            )
        })
    }

    fn counters(&self, user: UserId) -> BoxFuture<'_, (u64, u64)> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let alive = |list: Option<&Vec<UserId>>| {
                list.map_or(0, |list| {
                    list.iter()
                        .filter(|other| !state.deleted.contains(other))
                        .count() as u64
                })
            };
            (
                alive(state.followers.get(&user)),
                alive(state.following.get(&user)),
            )
        })
    }
}
