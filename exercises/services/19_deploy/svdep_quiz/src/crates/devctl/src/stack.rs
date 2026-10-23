use crate::backend::Backend;
use crate::graph::{ServiceSpec, StackError, instances_of, validate};
use futures::future::{join_all, try_join_all};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct Config {
    /// Сколько ждать готовности экземпляра.
    pub ready_timeout: Duration,
    /// Сколько ждать завершения после SIGTERM, прежде чем добить.
    pub grace: Duration,
    /// Сколько ждать, пока экземпляр доработает начатые запросы после выхода из круга.
    pub drain_timeout: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config { ready_timeout: Duration::from_secs(30), grace: Duration::from_secs(5), drain_timeout: Duration::from_secs(5) }
    }
}

pub struct Stack<B: Backend> {
    backend: B,
    specs: Vec<ServiceSpec>,
    cfg: Config,
    /// Круг балансировщика по сервисам: какие экземпляры принимают запросы.
    rotation: Mutex<HashMap<String, Vec<String>>>,
}

/// Ждёт готовности экземпляра, опрашивая каждые 100 мс. `false`, если не дождался за `timeout`.
pub async fn wait_ready<B: Backend>(backend: &B, instance: &str, timeout: Duration) -> bool {
    let begun = tokio::time::Instant::now();
    while !backend.ready(instance).await {
        if begun.elapsed() >= timeout {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    true
}

/// SIGTERM, ожидание до `grace` (опрос каждые 50 мс), затем SIGKILL. `true`, если пришлось добивать.
pub async fn stop_instance<B: Backend>(backend: &B, instance: &str, grace: Duration) -> bool {
    backend.term(instance).await;
    let begun = tokio::time::Instant::now();
    while !backend.exited(instance).await {
        if begun.elapsed() >= grace {
            backend.kill(instance).await;
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

impl<B: Backend> Stack<B> {
    pub fn new(backend: B, specs: Vec<ServiceSpec>, cfg: Config) -> Stack<B> {
        Stack { backend, specs, cfg, rotation: Mutex::new(HashMap::new()) }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Экземпляры сервиса, которые сейчас принимают запросы.
    pub fn serving(&self, service: &str) -> Vec<String> {
        self.rotation.lock().unwrap().get(service).cloned().unwrap_or_default()
    }

    fn enter(&self, service: &str, instance: &str) {
        let mut rotation = self.rotation.lock().unwrap();
        let list = rotation.entry(service.to_string()).or_default();
        if !list.iter().any(|i| i == instance) {
            list.push(instance.to_string());
        }
    }

    fn leave(&self, service: &str, instance: &str) {
        if let Some(list) = self.rotation.lock().unwrap().get_mut(service) {
            list.retain(|i| i != instance);
        }
    }

    /// Поднимает всё: сервис стартует (все его экземпляры вместе), когда все зависимости готовы; независимые стартуют одновременно.
    /// Готовый экземпляр входит в круг. Если какой-то экземпляр не стал готовым за `ready_timeout`, всё уже запущенное останавливается
    /// и возвращается `NotReady(имя экземпляра)`. Ошибки графа возвращаются до запуска чего-либо.
    pub async fn up(&self) -> Result<(), StackError> {
        validate(&self.specs)?;
        todo!()
    }

    /// Останавливает всё в обратном порядке: сервис получает SIGTERM только после того, как остановились все зависимые от него.
    /// Экземпляры одного сервиса останавливаются вместе, зависшие добиваются через `grace`. Выходит из круга всё.
    pub async fn down(&self) {
        todo!()
    }

    /// Выкат одного сервиса по экземплярам, по одному: выйти из круга, дождаться доработки начатых запросов (до `drain_timeout`),
    /// остановить, запустить, дождаться готовности и только тогда вернуться в круг. Остальные сервисы не трогаются.
    pub async fn restart(&self, service: &str) -> Result<(), StackError> {
        let _ = service;
        todo!()
    }
}
