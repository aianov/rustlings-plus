use std::collections::{BTreeSet, HashMap, HashSet};

use tokio::sync::Mutex;

use super::{BoxFuture, Opening, RefreshError, SessionRepository, SessionRow};

#[derive(Default)]
struct State {
    rows: HashMap<String, SessionRow>,
    revoked: HashSet<u64>,
    denied: HashMap<String, u64>,
    next_family: u64,
}

impl State {
    fn revoke(&mut self, family: u64, leeway: u64) {
        self.revoked.insert(family);
        let jtis: Vec<(String, u64)> = self
            .rows
            .values()
            .filter(|row| row.family == family)
            .map(|row| (row.access_jti.clone(), row.access_exp + leeway))
            .collect();
        self.denied.extend(jtis);
    }
}

pub struct MemorySessions {
    state: Mutex<State>,
    leeway: u64,
}

impl MemorySessions {
    pub fn new(leeway: u64) -> MemorySessions {
        MemorySessions {
            state: Mutex::default(),
            leeway,
        }
    }
}

impl SessionRepository for MemorySessions {
    fn open(&self, user_id: u64, opening: Opening) -> BoxFuture<'_, SessionRow> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            state.next_family += 1;
            let row = SessionRow {
                family: state.next_family,
                user_id,
                used: false,
                expires: opening.expires,
                access_jti: opening.access_jti,
                access_exp: opening.access_exp,
            };
            state.rows.insert(opening.refresh_hash, row.clone());
            row
        })
    }

    fn rotate(
        &self,
        old_hash: String,
        now: u64,
        opening: Opening,
    ) -> BoxFuture<'_, Result<SessionRow, RefreshError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let Some(old) = state.rows.get(&old_hash).cloned() else {
                return Err(RefreshError::Unknown);
            };
            if state.revoked.contains(&old.family) {
                return Err(RefreshError::Revoked);
            }
            if old.used {
                state.revoke(old.family, self.leeway);
                return Err(RefreshError::Reused);
            }
            if now >= old.expires {
                return Err(RefreshError::Expired);
            }
            if let Some(row) = state.rows.get_mut(&old_hash) {
                row.used = true;
            }
            let row = SessionRow {
                family: old.family,
                user_id: old.user_id,
                used: false,
                expires: opening.expires,
                access_jti: opening.access_jti,
                access_exp: opening.access_exp,
            };
            state.rows.insert(opening.refresh_hash, row.clone());
            Ok(row)
        })
    }

    fn family_of(&self, jti: String) -> BoxFuture<'_, Option<SessionRow>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            state
                .rows
                .values()
                .find(|row| row.access_jti == jti)
                .cloned()
        })
    }

    fn revoke_family(&self, family: u64) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.state.lock().await.revoke(family, self.leeway);
        })
    }

    fn revoke_user(&self, user_id: u64, keep: Option<u64>) -> BoxFuture<'_, Vec<u64>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let families: BTreeSet<u64> = state
                .rows
                .values()
                .filter(|row| row.user_id == user_id && Some(row.family) != keep)
                .map(|row| row.family)
                .collect();
            for family in &families {
                state.revoke(*family, self.leeway);
            }
            families.into_iter().collect()
        })
    }

    fn row(&self, refresh_hash: String) -> BoxFuture<'_, Option<SessionRow>> {
        Box::pin(async move { self.state.lock().await.rows.get(&refresh_hash).cloned() })
    }

    fn deny(&self, jti: String, until: u64) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.state.lock().await.denied.insert(jti, until);
        })
    }

    fn is_denied(&self, jti: String) -> BoxFuture<'_, bool> {
        Box::pin(async move { self.state.lock().await.denied.contains_key(&jti) })
    }

    fn purge(&self, now: u64) -> BoxFuture<'_, usize> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let before = state.denied.len() + state.rows.len();
            state.denied.retain(|_, until| *until > now);
            state.rows.retain(|_, row| row.expires > now);
            before - state.denied.len() - state.rows.len()
        })
    }
}
