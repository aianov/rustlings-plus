// Лимит «5 сообщений в секунду» фиксированными окнами пропускает до 10 сообщений вокруг границы окна: 5 в
// конце одного и 5 в начале следующего. Скользящее окно считает события за последние `window_ms` от текущего
// момента. Храни моменты событий в sorted set (счёт это время), удаляй устаревшие, считай оставшиеся. Всё это
// должно быть атомарным: двадцать одновременных запросов не должны проскочить мимо подсчёта. Время
// передаётся аргументом в миллисекундах.
// TODO: `allow` возвращает true и записывает событие, если в окне меньше `limit` событий.

use course_support::redis::{Client, Error};

async fn allow(c: &Client, ns: &str, user: i64, now_ms: i64, limit: i64, window_ms: i64) -> Result<bool, Error> {
    let _ = (c, ns, user, now_ms, limit, window_ms);
    todo!("скрипт со скользящим окном")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn sliding_window_has_no_boundary_burst() {
        let s = Scope::new().await;
        let c = s.client();
        let ns = s.prefix();
        let mut got = Vec::new();
        for t in [995, 996, 997, 998, 999, 1000, 1001, 1002, 1996, 1997] {
            got.push(allow(&c, ns, 7, t, 5, 1000).await.expect("лимит"));
        }
        assert_eq!(got, [true, true, true, true, true, false, false, false, true, true], "в любую секунду не больше 5, а через секунду после первых снова можно");
    }

    #[tokio::test]
    async fn concurrent_requests_respect_the_limit() {
        let s = Scope::new().await;
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (c, ns) = (s.client(), s.prefix().to_string());
            tasks.push(tokio::spawn(async move { allow(&c, &ns, 9, 5000, 5, 1000).await.expect("лимит") }));
        }
        let mut allowed = 0;
        for t in tasks {
            if t.await.expect("задача") {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 5, "из двадцати одновременных прошло ровно пять");
    }
}
