use std::future::{Future, pending};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, PartialEq)]
struct Stuck {
    layer: &'static str,
}

#[derive(Clone, Copy)]
struct Hang(Option<&'static str>);

async fn pass(layer: &'static str, hang: Hang, work_ms: u64) -> u64 {
    if hang.0 == Some(layer) {
        pending::<()>().await;
    }
    sleep(Duration::from_millis(work_ms)).await;
    work_ms
}

async fn auth(hang: Hang) -> u64 {
    pass("auth", hang, 5).await
}

async fn cache(hang: Hang) -> u64 {
    pass("cache", hang, 10).await
}

async fn database(hang: Hang) -> u64 {
    pass("database", hang, 40).await
}

async fn render(hang: Hang) -> u64 {
    pass("render", hang, 20).await
}

async fn layer<T>(name: &'static str, work: impl Future<Output = T>) -> Result<T, Stuck> {
    tokio::time::timeout(Duration::from_secs(1), work).await.map_err(|_| Stuck { layer: name })
}

async fn handle(hang: Hang) -> Result<u64, Stuck> {
    let a = layer("auth", auth(hang)).await?;
    let c = layer("cache", cache(hang)).await?;
    let d = layer("database", database(hang)).await?;
    let r = layer("render", render(hang)).await?;
    Ok(a + c + d + r)
}

#[tokio::main]
async fn main() {
    println!("{:?}", handle(Hang(None)).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    async fn guarded(hang: Hang) -> Result<u64, Stuck> {
        tokio::time::timeout(Duration::from_secs(60), handle(hang)).await.expect("запрос не вернулся даже за минуту: без именованных таймаутов он висит вечно")
    }

    #[tokio::test(start_paused = true)]
    async fn a_healthy_request_goes_through_all_layers() {
        let started = Instant::now();
        let answer = guarded(Hang(None)).await;
        assert_eq!(answer, Ok(75), "сумма работы слоёв 5 + 10 + 40 + 20");
        assert_eq!(started.elapsed(), Duration::from_millis(75), "здоровый запрос идёт по слоям последовательно");
    }

    #[tokio::test(start_paused = true)]
    async fn every_layer_is_named_when_it_hangs() {
        for (name, before_ms) in [("auth", 0), ("cache", 5), ("database", 15), ("render", 55)] {
            let started = Instant::now();
            let answer = guarded(Hang(Some(name))).await;
            assert_eq!(answer, Err(Stuck { layer: name }), "зависает слой {name}: ошибка должна называть именно его");
            assert_eq!(
                started.elapsed(),
                Duration::from_millis(before_ms + 1000),
                "слой {name}: секунда отсчитывается с его начала (до него работа заняла {before_ms} мс), а не с начала запроса"
            );
        }
    }
}
