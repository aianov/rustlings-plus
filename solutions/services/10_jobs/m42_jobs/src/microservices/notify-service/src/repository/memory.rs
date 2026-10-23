use std::collections::HashMap;

use contracts::UserId;
use tokio::sync::Mutex;

use super::{BoxFuture, NoticeRepository, Recorded};
use crate::common::{Draft, Notice};

#[derive(Default)]
struct State {
    next_id: u64,
    by_user: HashMap<UserId, Vec<Notice>>,
}

#[derive(Default)]
pub struct MemoryNotices {
    state: Mutex<State>,
}

impl MemoryNotices {
    pub fn new() -> MemoryNotices {
        MemoryNotices::default()
    }
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl NoticeRepository for MemoryNotices {
    fn record(&self, draft: Draft) -> BoxFuture<'_, Recorded> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let known = state.by_user.get(&draft.to);
            if known.is_some_and(|notices| notices.iter().any(|notice| notice.same(&draft))) {
                return Recorded::Duplicate;
            }
            state.next_id += 1;
            let notice = Notice {
                id: state.next_id,
                to: draft.to,
                kind: draft.kind,
                actor: draft.actor,
                post: draft.post,
                read: false,
                count: draft.count,
            };
            state
                .by_user
                .entry(draft.to)
                .or_default()
                .push(notice.clone());
            Recorded::Stored(notice)
        })
    }

    fn unread(&self, user: UserId) -> BoxFuture<'_, u32> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let notices = state.by_user.get(&user).map_or(&[][..], Vec::as_slice);
            count(notices.iter().filter(|notice| !notice.read).count())
        })
    }

    fn list(&self, user: UserId, limit: usize) -> BoxFuture<'_, Vec<Notice>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let notices = state.by_user.get(&user).map_or(&[][..], Vec::as_slice);
            notices.iter().rev().take(limit).cloned().collect()
        })
    }

    fn unread_counts(&self) -> BoxFuture<'_, Vec<(UserId, u32)>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let mut counts: Vec<(UserId, u32)> = state
                .by_user
                .iter()
                .map(|(user, notices)| (*user, count(notices.iter().filter(|n| !n.read).count())))
                .filter(|(_, unread)| *unread > 0)
                .collect();
            counts.sort_unstable();
            counts
        })
    }

    fn mark_read(&self, user: UserId) -> BoxFuture<'_, u32> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let Some(notices) = state.by_user.get_mut(&user) else {
                return 0;
            };
            let mut marked = 0;
            for notice in notices.iter_mut().filter(|notice| !notice.read) {
                notice.read = true;
                marked += 1;
            }
            marked
        })
    }
}
