use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
struct Journal(Arc<Mutex<Vec<String>>>);

impl Journal {
    fn add(&self, line: impl Into<String>) {
        self.0.lock().unwrap().push(line.into());
    }

    fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

async fn work(journal: &Journal, name: &str) -> usize {
    journal.add(format!("{name}: работа"));
    tokio::time::sleep(Duration::from_millis(1)).await;
    name.chars().count()
}

fn traced<F: Future>(journal: &Journal, name: &str, fut: F) -> impl Future<Output = F::Output> {
    let journal = journal.clone();
    let name = name.to_string();
    async move {
        journal.add(format!("{name}: старт"));
        let output = fut.await;
        journal.add(format!("{name}: конец"));
        output
    }
}

#[tokio::main]
async fn main() {
    let journal = Journal::default();
    traced(&journal, "аня", work(&journal, "аня")).await;
    for line in journal.lines() {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn nothing_happens_before_await() {
        let journal = Journal::default();
        let fut = traced(&journal, "a", work(&journal, "a"));
        assert!(
            journal.lines().is_empty(),
            "журнал не пуст до .await: тело async fn не должно выполняться при вызове, только при ожидании"
        );
        let size = fut.await;
        assert_eq!(size, 1, "traced должна вернуть то, что вернула работа");
        assert_eq!(
            journal.lines(),
            ["a: старт", "a: работа", "a: конец"],
            "порядок в журнале: старт, работа, конец"
        );
    }

    #[tokio::test]
    async fn order_follows_await_not_creation() {
        let journal = Journal::default();
        let first = traced(&journal, "первая", work(&journal, "первая"));
        let second = traced(&journal, "вторая", work(&journal, "вторая"));
        second.await;
        first.await;
        assert_eq!(
            journal.lines(),
            [
                "вторая: старт",
                "вторая: работа",
                "вторая: конец",
                "первая: старт",
                "первая: работа",
                "первая: конец"
            ],
            "Future выполняется там, где его ждут, а не там, где создали"
        );
    }
}
