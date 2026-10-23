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
        let channels: HashMap<&str, (watch::Sender<bool>, watch::Receiver<bool>)> = self.specs.iter().map(|s| (s.name, watch::channel(false))).collect();
        let result = try_join_all(self.specs.iter().map(|s| {
            let channels = &channels;
            async move {
                for dep in &s.deps {
                    let mut rx = channels[dep].1.clone();
                    let _ = rx.wait_for(|ready| *ready).await;
                }
                let instances = instances_of(s);
                join_all(instances.iter().map(|i| self.backend.start(i))).await;
                let ready = join_all(instances.iter().map(|i| wait_ready(&self.backend, i, self.cfg.ready_timeout))).await;
                if let Some(bad) = ready.iter().position(|ok| !ok) {
                    return Err(StackError::NotReady(instances[bad].clone()));
                }
                for i in &instances {
                    self.enter(s.name, i);
                }
                let _ = channels[s.name].0.send(true);
                Ok(())
            }
        }))
        .await;
        if let Err(e) = result {
            self.down().await;
            return Err(e);
        }
        Ok(())
    }

    /// Останавливает всё в обратном порядке: сервис получает SIGTERM только после того, как остановились все зависимые от него.
    /// Экземпляры одного сервиса останавливаются вместе, зависшие добиваются через `grace`. Выходит из круга всё.
    pub async fn down(&self) {
        let channels: HashMap<&str, (watch::Sender<bool>, watch::Receiver<bool>)> = self.specs.iter().map(|s| (s.name, watch::channel(false))).collect();
        join_all(self.specs.iter().map(|s| {
            let channels = &channels;
            async move {
                for dependent in self.specs.iter().filter(|d| d.deps.contains(&s.name)) {
                    let mut rx = channels[dependent.name].1.clone();
                    let _ = rx.wait_for(|stopped| *stopped).await;
                }
                let instances = instances_of(s);
                for i in &instances {
                    self.leave(s.name, i);
                }
                join_all(instances.iter().map(|i| stop_instance(&self.backend, i, self.cfg.grace))).await;
                let _ = channels[s.name].0.send(true);
            }
        }))
        .await;
    }

    /// Выкат одного сервиса по экземплярам, по одному: выйти из круга, дождаться доработки начатых запросов (до `drain_timeout`),
    /// остановить, запустить, дождаться готовности и только тогда вернуться в круг. Остальные сервисы не трогаются.
    pub async fn restart(&self, service: &str) -> Result<(), StackError> {
        let spec = self.specs.iter().find(|s| s.name == service).ok_or_else(|| StackError::Unknown(service.to_string()))?;
        for instance in instances_of(spec) {
            self.leave(spec.name, &instance);
            let drained = async {
                while self.backend.in_flight(&instance).await > 0 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            };
            let _ = tokio::time::timeout(self.cfg.drain_timeout, drained).await;
            stop_instance(&self.backend, &instance, self.cfg.grace).await;
            self.backend.start(&instance).await;
            if !wait_ready(&self.backend, &instance, self.cfg.ready_timeout).await {
                return Err(StackError::NotReady(instance));
            }
            self.enter(spec.name, &instance);
        }
        Ok(())
    }
}
