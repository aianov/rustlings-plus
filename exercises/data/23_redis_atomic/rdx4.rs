// Сервис держит один общий клиент на всё приложение (копии клиента делят одно соединение). Перевод очков
// отправляет MULTI, команды и EXEC отдельными вызовами. Под нагрузкой в транзакцию попадают чужие команды:
// они идут по тому же соединению между твоими MULTI и EXEC и встают в очередь твоей транзакции, а их авторы
// получают QUEUED вместо ответа. Почини `transfer` так, чтобы транзакция на общем клиенте не смешивалась с
// чужими командами.

use course_support::redis::{Client, Error};

async fn transfer(c: &Client, from: &str, to: &str, amount: i64) -> Result<(), Error> {
    let n = amount.to_string();
    c.cmd(&["MULTI"]).await?;
    tokio::task::yield_now().await;
    c.cmd(&["DECRBY", from, &n]).await?;
    tokio::task::yield_now().await;
    c.cmd(&["INCRBY", to, &n]).await?;
    tokio::task::yield_now().await;
    c.cmd(&["EXEC"]).await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn shared_client_keeps_transactions_apart() {
        let s = Scope::new().await;
        let c = s.client();
        let (a, b, hits) = (s.key("points:a"), s.key("points:b"), s.key("hits"));
        s.cmd(&["MSET", &a, "1000", &b, "1000"]).await.expect("балансы");
        let mut tasks = Vec::new();
        for i in 0..40 {
            let (c, a, b, hits) = (c.clone(), a.clone(), b.clone(), hits.clone());
            tasks.push(tokio::spawn(async move {
                if i % 2 == 0 {
                    transfer(&c, &a, &b, 1).await.map(|_| ())
                } else {
                    let v = c.cmd(&["INCR", &hits]).await?;
                    if v.int().is_none() {
                        return Err(course_support::redis::Error::Protocol(format!("вместо числа пришло {v:?}: чужая команда попала в транзакцию")));
                    }
                    Ok(())
                }
            }));
        }
        for t in tasks {
            t.await.expect("задача").expect("ни одна команда не должна сломаться");
        }
        let got = s.cmd(&["MGET", &a, &b, &hits]).await.expect("MGET").texts();
        assert_eq!(got, ["980", "1020", "20"], "20 переводов и 20 независимых INCR, всё на своих местах");
    }
}
