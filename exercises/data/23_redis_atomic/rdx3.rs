// Продажа билетов: осталось 10. Покупка читает остаток, и если он больше нуля, списывает один. Под нагрузкой
// продаётся больше, чем было: двадцать покупателей одновременно прочитали «есть», и каждый списал. Почини
// оптимистичной блокировкой: следи за ключом остатка (WATCH), прочитай его, и если в транзакцию между
// чтением и EXEC кто-то вмешался, EXEC ничего не выполнит, а ты попробуешь снова. Каждой покупке нужно своё
// соединение: WATCH и MULTI живут на соединении. Возвращай true, если билет куплен, и false, если их не осталось.

use course_support::redis::{Client, Error};
use std::time::Duration;

async fn buy(addr: &str, key: &str) -> Result<bool, Error> {
    let c = Client::connect_to(addr).await?;
    let left = c.cmd(&["GET", key]).await?.int().unwrap_or(0);
    if left <= 0 {
        return Ok(false);
    }
    tokio::time::sleep(Duration::from_millis(5)).await;
    c.cmd(&["DECR", key]).await?;
    Ok(true)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn never_oversold() {
        let s = Scope::new().await;
        let key = s.key("tickets:concert");
        s.cmd(&["SET", &key, "10"]).await.expect("остаток");
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (addr, key) = (s.addr(), key.clone());
            tasks.push(tokio::spawn(async move { buy(&addr, &key).await.expect("покупка") }));
        }
        let mut sold = 0;
        for t in tasks {
            if t.await.expect("задача") {
                sold += 1;
            }
        }
        assert_eq!(sold, 10, "продано ровно 10 билетов из 10");
        assert_eq!(s.cmd(&["GET", &key]).await.expect("GET").int(), Some(0), "остаток ноль, а не меньше");
    }
}
