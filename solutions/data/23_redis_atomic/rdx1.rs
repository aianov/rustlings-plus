use course_support::redis::{Client, Error};

async fn transfer(c: &Client, from: &str, to: &str, amount: i64) -> Result<(i64, i64), Error> {
    let n = amount.to_string();
    let replies = c.pipeline(&[&["MULTI"], &["DECRBY", from, &n], &["INCRBY", to, &n], &["EXEC"]]).await?;
    let results = replies.last().map(|v| v.list()).unwrap_or_default();
    match (results.first().and_then(|v| v.int()), results.get(1).and_then(|v| v.int())) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => Err(Error::Protocol(format!("неожиданный ответ EXEC: {replies:?}"))),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use course_support::redis::{FakeServer, Step};

    #[tokio::test]
    async fn real_transfer() {
        let s = Scope::new().await;
        let (a, b) = (s.key("points:1"), s.key("points:2"));
        s.cmd(&["MSET", &a, "100", &b, "50"]).await.expect("начальные балансы");
        assert_eq!(transfer(&s.client(), &a, &b, 30).await.expect("перевод"), (70, 80), "новые балансы из ответа EXEC");
    }

    #[tokio::test]
    async fn commands_go_inside_multi_exec() {
        let server = FakeServer::start(|_, cmd| {
            let reply: &[u8] = match cmd[0].as_str() {
                "MULTI" => b"+OK\r\n",
                "EXEC" => b"*2\r\n:70\r\n:80\r\n",
                _ => b"+QUEUED\r\n",
            };
            vec![Step::Write(reply.to_vec())]
        })
        .await;
        let c = course_support::redis::Client::connect_to(&server.addr()).await.expect("подключение");
        assert_eq!(transfer(&c, "a", "b", 30).await.expect("перевод"), (70, 80), "балансы берутся из массива EXEC, а не из ответов QUEUED");
        let names: Vec<String> = server.commands().iter().map(|c| c[0].clone()).collect();
        assert_eq!(names, ["MULTI", "DECRBY", "INCRBY", "EXEC"], "обе команды внутри MULTI ... EXEC");
    }
}
