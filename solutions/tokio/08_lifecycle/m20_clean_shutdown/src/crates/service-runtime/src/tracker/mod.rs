use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::watch;
use tokio::task::{AbortHandle, JoinHandle};

#[derive(Default)]
struct Running {
    tasks: HashMap<u64, AbortHandle>,
    early: HashSet<u64>,
}

#[derive(Default)]
struct Inner {
    running: Mutex<Running>,
    next: AtomicU64,
    closed: AtomicBool,
    changed: watch::Sender<usize>,
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, Running> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct Exit {
    inner: Arc<Inner>,
    id: u64,
}

impl Drop for Exit {
    fn drop(&mut self) {
        let left = {
            let mut running = self.inner.lock();
            if running.tasks.remove(&self.id).is_none() {
                running.early.insert(self.id);
            }
            running.tasks.len()
        };
        self.inner.changed.send_replace(left);
    }
}

#[derive(Clone, Default)]
pub struct TaskTracker {
    inner: Arc<Inner>,
}

impl TaskTracker {
    pub fn new() -> TaskTracker {
        TaskTracker::default()
    }

    pub fn spawn<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let id = self.inner.next.fetch_add(1, Ordering::Relaxed);
        let exit = Exit {
            inner: Arc::clone(&self.inner),
            id,
        };
        let handle = tokio::spawn(async move {
            let _exit = exit;
            task.await
        });
        let count = {
            let mut running = self.inner.lock();
            if !running.early.remove(&id) {
                running.tasks.insert(id, handle.abort_handle());
            }
            running.tasks.len()
        };
        self.inner.changed.send_replace(count);
        handle
    }

    pub fn len(&self) -> usize {
        self.inner.lock().tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn close(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        self.inner.changed.send_modify(|_| {});
    }

    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
    }

    pub fn abort_all(&self) -> usize {
        let running = self.inner.lock();
        for handle in running.tasks.values() {
            handle.abort();
        }
        running.tasks.len()
    }

    pub async fn wait(&self) {
        let mut changed = self.inner.changed.subscribe();
        loop {
            if self.is_closed() && self.is_empty() {
                return;
            }
            if changed.changed().await.is_err() {
                return;
            }
        }
    }
}
