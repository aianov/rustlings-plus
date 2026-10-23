use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

struct Repo;

impl Repo {
    async fn save(&self, _id: u32) {
        sleep(Duration::from_millis(5)).await;
    }
}

struct Service {
    audit: Arc<Mutex<Vec<String>>>,
    repo: Repo,
}

impl Service {
    fn new() -> Service {
        Service { audit: Arc::new(Mutex::new(Vec::new())), repo: Repo }
    }

    fn audit_log(&self) -> Vec<String> {
        self.audit.lock().unwrap().clone()
    }

    async fn touch(&self, id: u32) {
        self.repo.save(id).await;
        self.audit.lock().unwrap().push(format!("touch {id}"));
    }
}

impl Service {
    async fn register(&self, id: u32) {
        self.audit.lock().unwrap().push(format!("register {id}"));
        self.repo.save(id).await;
        self.audit.lock().unwrap().push(format!("registered {id}"));
    }

    async fn rename(&self, id: u32, name: &str) {
        let label: Arc<str> = Arc::from(name);
        self.repo.save(id).await;
        self.audit.lock().unwrap().push(format!("rename {id} to {label}"));
    }
}

#[tokio::main]
async fn main() {
    let service = Service::new();
    service.register(1).await;
    service.rename(1, "Аня").await;
    println!("{:?}", service.audit_log());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>(_future: &T) {}

    #[test]
    fn every_method_future_is_send() {
        let service = Service::new();
        assert_send(&service.touch(1));
        assert_send(&service.register(1));
        assert_send(&service.rename(1, "x"));
    }

    #[tokio::test]
    async fn audit_keeps_its_order() {
        let service = Service::new();
        service.register(1).await;
        service.rename(1, "Аня").await;
        service.touch(1).await;
        assert_eq!(
            service.audit_log(),
            vec!["register 1", "registered 1", "rename 1 to Аня", "touch 1"],
            "записи аудита должны идти в прежнем порядке, по одной на шаг"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn methods_can_be_spawned() {
        let service = Arc::new(Service::new());
        let tasks: Vec<_> = (0..8)
            .map(|id| {
                let service = Arc::clone(&service);
                tokio::spawn(async move {
                    service.register(id).await;
                    service.rename(id, "n").await;
                })
            })
            .collect();
        for task in tasks {
            task.await.unwrap();
        }
        let log = service.audit_log();
        assert_eq!(log.len(), 24, "по три записи на каждый из восьми id: {log:?}");
        for id in 0..8 {
            let first = log.iter().position(|line| *line == format!("register {id}")).expect("запись о начале");
            let second = log.iter().position(|line| *line == format!("registered {id}")).expect("запись об окончании");
            assert!(first < second, "для id {id} запись об окончании регистрации стоит раньше записи о начале");
        }
    }
}
