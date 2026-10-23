// Шлюз держит реестр открытых соединений: у каждого клиента там лежит очередь исходящих сообщений.
// Запись добавляют при подключении, а убирают руками в конце обработчика. Если обработчик вышел через `?` или упал,
// запись остаётся навсегда: реестр пухнет, а рассылка шлёт сообщения в очереди, которые никто не читает.
// TODO: Сделай реестр, из которого запись исчезает сама, как бы ни закончилось соединение. Тесты проверяют три выхода и ещё одну гадость.

#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::mpsc;

type ConnId = u64;

#[derive(Clone, Default)]
struct Registry {
    conns: Arc<Mutex<HashMap<ConnId, mpsc::Sender<String>>>>,
    next: Arc<AtomicU64>,
}

struct Guard {
    registry: Registry,
    id: ConnId,
}

impl Guard {
    fn id(&self) -> ConnId {
        self.id
    }
}

impl Registry {
    fn lock(&self) -> MutexGuard<'_, HashMap<ConnId, mpsc::Sender<String>>> {
        self.conns.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn register(&self, tx: mpsc::Sender<String>) -> Guard {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.lock().insert(id, tx);
        Guard { registry: self.clone(), id }
    }

    fn len(&self) -> usize {
        self.lock().len()
    }

    fn send(&self, id: ConnId, message: &str) -> bool {
        let tx = self.lock().get(&id).cloned();
        match tx {
            Some(tx) => tx.try_send(message.to_string()).is_ok(),
            None => false,
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.registry.lock().remove(&self.id);
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_removes_entry() {
        let registry = Registry::default();
        let (tx, _rx) = mpsc::channel(4);
        let guard = registry.register(tx);
        assert_eq!(registry.len(), 1, "после register в реестре должна быть одна запись");
        drop(guard);
        assert_eq!(registry.len(), 0, "гард умер, а запись осталась: реестр обязан чиститься в Drop");
    }

    fn handle(registry: &Registry, fail: bool) -> Result<(), String> {
        let (tx, _rx) = mpsc::channel(4);
        let _guard = registry.register(tx);
        if fail {
            Err("клиент прислал мусор".to_string())?;
        }
        Ok(())
    }

    #[test]
    fn early_return_cleans() {
        let registry = Registry::default();
        assert!(handle(&registry, true).is_err());
        assert!(handle(&registry, false).is_ok());
        assert_eq!(registry.len(), 0, "обработчик вышел (через `?` или нормально), а запись осталась");
    }

    #[tokio::test]
    async fn panic_cleans() {
        let registry = Registry::default();
        let inner = registry.clone();
        let task = tokio::spawn(async move {
            let (tx, _rx) = mpsc::channel(4);
            let _guard = inner.register(tx);
            panic!("обработчик упал");
        });
        assert!(task.await.is_err(), "задача должна была упасть");
        assert_eq!(registry.len(), 0, "после паники запись должна исчезнуть: Drop срабатывает и при раскрутке стека");
    }

    #[test]
    fn ids_are_unique_and_send_reaches_only_target() {
        let registry = Registry::default();
        let (tx1, mut rx1) = mpsc::channel(4);
        let (tx2, mut rx2) = mpsc::channel(4);
        let g1 = registry.register(tx1);
        let g2 = registry.register(tx2);
        assert_ne!(g1.id(), g2.id(), "у соединений должны быть разные id");
        assert!(registry.send(g2.id(), "привет"));
        assert_eq!(rx2.try_recv().as_deref(), Ok("привет"));
        assert!(rx1.try_recv().is_err(), "сообщение для второго соединения попало в первое");
        let gone = g2.id();
        drop(g2);
        assert!(!registry.send(gone, "эхо"), "отправка в закрытое соединение должна вернуть false");
    }

    #[test]
    fn full_queue_is_not_success() {
        let registry = Registry::default();
        let (tx, _rx) = mpsc::channel(1);
        let guard = registry.register(tx);
        assert!(registry.send(guard.id(), "раз"));
        assert!(!registry.send(guard.id(), "два"), "очередь на одно место уже занята: send не должен ждать и обязан вернуть false");
    }

    #[test]
    fn poisoned_lock_does_not_break_cleanup() {
        let registry = Registry::default();
        let (tx, _rx) = mpsc::channel(4);
        let guard = registry.register(tx);
        let other = registry.clone();
        let crashed = std::thread::spawn(move || {
            let _held = other.conns.lock().unwrap();
            panic!("поток умер, пока держал замок реестра");
        })
        .join();
        assert!(crashed.is_err());
        let (tx2, _rx2) = mpsc::channel(4);
        let second = registry.register(tx2);
        assert_eq!(registry.len(), 2, "реестр после отравления замка должен продолжать работать");
        drop(guard);
        drop(second);
        assert_eq!(registry.len(), 0, "очистка в Drop должна переживать отравленный замок, а не паниковать");
    }
}
