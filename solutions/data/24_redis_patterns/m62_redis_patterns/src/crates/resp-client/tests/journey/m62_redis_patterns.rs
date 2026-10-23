use resp_client::{Client, RedisConfig, Script, cmd, key};

const ADD: &str = "return redis.call('INCRBY', KEYS[1], ARGV[1])";

#[tokio::test]
async fn scripts_run_by_sha_and_fall_back_to_eval() {
    let client = Client::connect(RedisConfig::from_env());
    client.ping().await.expect("Redis курса на 127.0.0.1:56379");
    let counter = key("test", "script", std::process::id());
    client.del(&[counter.clone()]).await.expect("чисто");
    let script = Script::new(ADD);
    let keys = [counter.clone()];
    let first = client
        .eval(&script, &keys, &[String::from("2")])
        .await
        .expect("EVALSHA");
    assert_eq!(first.as_int(), Some(2));
    client
        .query(cmd("SCRIPT").arg("FLUSH"))
        .await
        .expect("кэш скриптов очищен");
    let second = client
        .eval(&script, &keys, &[String::from("3")])
        .await
        .expect("NOSCRIPT, затем EVAL");
    assert_eq!(second.as_int(), Some(5));
    let third = client
        .eval(&script, &keys, &[String::from("1")])
        .await
        .expect("снова по sha");
    assert_eq!(third.as_int(), Some(6));
    client.del(&[counter]).await.expect("уборка");
}
