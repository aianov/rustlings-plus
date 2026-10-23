// Async-блок это Future как значение: его можно собрать сейчас, положить в Vec и запустить потом.
// Но блок без move только заимствует локальные переменные, а работать ему придётся уже
// после выхода из функции. Компилятор это видит и ругается.
// TODO: Почини обе функции так, чтобы Future можно было вернуть и запустить позже.
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn greet_later(name: &str) -> impl Future<Output = String> {
    let name = name.to_string();
    async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        format!("Привет, {name}!")
    }
}

fn build_queue(counter: &Arc<AtomicUsize>, names: &[&str]) -> Vec<impl Future<Output = String>> {
    names
        .iter()
        .map(|name| {
            let counter = Arc::clone(counter);
            let name = name.to_string();
            async {
                let position = counter.fetch_add(1, Ordering::SeqCst) + 1;
                format!("{position}. {name}")
            }
        })
        .collect()
}

#[tokio::main]
async fn main() {
    let counter = Arc::new(AtomicUsize::new(0));
    for job in build_queue(&counter, &["аня", "борис"]) {
        println!("{}", job.await);
    }
    println!("{}", greet_later("вера").await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn greeting_is_built_now_and_finished_later() {
        let greeting = greet_later("аня");
        assert_eq!(greeting.await, "Привет, аня!", "async move должен унести name внутрь блока");
    }

    #[tokio::test]
    async fn queue_runs_only_when_awaited() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut jobs = build_queue(&counter, &["аня", "борис", "вера"]);
        assert_eq!(
            counter.load(Ordering::SeqCst),
            0,
            "ничего не должно выполниться, пока Future не дождались"
        );
        let third = jobs.pop().unwrap();
        let first = jobs.remove(0);
        let second = jobs.pop().unwrap();
        assert_eq!(third.await, "1. вера", "номер выдаётся в момент запуска, а не создания");
        assert_eq!(first.await, "2. аня", "второй запущенный получает номер 2");
        assert_eq!(second.await, "3. борис", "третий запущенный получает номер 3");
        assert_eq!(counter.load(Ordering::SeqCst), 3, "счётчик общий для всех задач очереди");
    }
}
