use std::future::Future;
use std::time::Duration;
use tokio::runtime::Runtime;

async fn make_greeting(name: &str) -> String {
    tokio::time::sleep(Duration::from_millis(5)).await;
    format!("Привет, {name}!")
}

fn run<F: Future>(fut: F) -> F::Output {
    Runtime::new().expect("не удалось собрать рантайм").block_on(fut)
}

fn greeting_sync(name: &str) -> String {
    run(make_greeting(name))
}

fn main() {
    println!("{}", greeting_sync("мир"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn run_returns_what_the_future_returns() {
        assert_eq!(run(async { 21 * 2 }), 42, "run должна вернуть результат Future");
    }

    #[test]
    fn run_has_working_timers() {
        let started = Instant::now();
        run(async { tokio::time::sleep(Duration::from_millis(30)).await });
        assert!(
            started.elapsed() >= Duration::from_millis(30),
            "sleep внутри run не отработал: рантайм должен уметь таймеры"
        );
    }

    #[test]
    fn run_can_be_called_many_times() {
        assert_eq!(run(async { 1 }) + run(async { 2 }), 3, "run вызывается сколько угодно раз");
    }

    #[test]
    fn sync_greeting() {
        assert_eq!(greeting_sync("Аня"), "Привет, Аня!", "greeting_sync должна вернуть готовую строку без await у вызывающего");
    }
}
