// Token bucket для API: у пользователя ведро на `capacity` жетонов, которое пополняется со скоростью
// `per_sec` жетонов в секунду, но не выше вместимости. Запрос забирает жетон, если он есть. В отличие от окна,
// ведро разрешает короткий всплеск до вместимости и затем ровную скорость. Состояние (жетоны и момент
// последнего пополнения) храни в хеше, пересчёт и списание делай одним скриптом Lua. Время аргументом в мс.
// TODO: `take` возвращает true, если жетон списан.

use course_support::redis::{Client, Error};

async fn take(c: &Client, ns: &str, user: i64, now_ms: i64, capacity: i64, per_sec: i64) -> Result<bool, Error> {
    let _ = (c, ns, user, now_ms, capacity, per_sec);
    todo!("скрипт ведра")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn burst_then_steady_rate() {
        let s = Scope::new().await;
        let c = s.client();
        let ns = s.prefix();
        let mut got = Vec::new();
        for t in [0, 0, 0, 0, 500, 1000, 1000, 5000, 5000, 5000, 5000] {
            got.push(take(&c, ns, 7, t, 3, 1).await.expect("ведро"));
        }
        assert_eq!(
            got,
            [true, true, true, false, false, true, false, true, true, true, false],
            "всплеск до трёх, потом жетон в секунду, а за долгую паузу не больше вместимости"
        );
    }

    #[tokio::test]
    async fn concurrent_takes_are_counted() {
        let s = Scope::new().await;
        let mut tasks = Vec::new();
        for _ in 0..10 {
            let (c, ns) = (s.client(), s.prefix().to_string());
            tasks.push(tokio::spawn(async move { take(&c, &ns, 9, 100, 3, 1).await.expect("ведро") }));
        }
        let mut ok = 0;
        for t in tasks {
            if t.await.expect("задача") {
                ok += 1;
            }
        }
        assert_eq!(ok, 3, "из десяти одновременных жетоны получили трое");
    }
}
