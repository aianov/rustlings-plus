use async_nats::jetstream::Context;
use async_nats::jetstream::stream::{Config, Stream};

fn main() {}

pub async fn ensure_stream(js: &Context, config: Config) -> Result<Stream, String> {
    let mut stream = js.get_or_create_stream(config.clone()).await.map_err(|e| e.to_string())?;
    let existing = stream.info().await.map_err(|e| e.to_string())?;
    if existing.config.subjects != config.subjects {
        js.update_stream(config).await.map_err(|e| e.to_string())?;
        return js.get_stream(&existing.config.name).await.map_err(|e| e.to_string());
    }
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::stream::RetentionPolicy;
    use course_support::nats::{Scope, connect, js};

    fn config(scope: &Scope, subjects: &[&str]) -> Config {
        Config { name: scope.name("EVENTS"), subjects: subjects.iter().map(|s| scope.subject(s)).collect(), ..Default::default() }
    }

    async fn publish(js: &Context, subject: String, n: usize) {
        for i in 0..n {
            js.publish(subject.clone(), format!("e{i}").into()).await.unwrap().await.unwrap();
        }
    }

    async fn messages(js: &Context, name: &str) -> u64 {
        js.get_stream(name).await.unwrap().get_info().await.unwrap().state.messages
    }

    #[tokio::test]
    async fn first_start_creates_the_stream() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let cfg = config(&scope, &["post.>"]);
        scope.track(&cfg.name);
        let stream = ensure_stream(&js, cfg.clone()).await.expect("первый запуск");
        assert_eq!(stream.cached_info().config.subjects, cfg.subjects, "stream создан на нужные темы");
    }

    #[tokio::test]
    async fn restart_keeps_the_history() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let cfg = config(&scope, &["post.>"]);
        scope.track(&cfg.name);
        ensure_stream(&js, cfg.clone()).await.unwrap();
        publish(&js, scope.subject("post.liked"), 5).await;
        ensure_stream(&js, cfg.clone()).await.expect("перезапуск");
        ensure_stream(&js, cfg.clone()).await.expect("ещё один перезапуск");
        assert_eq!(messages(&js, &cfg.name).await, 5, "после перезапусков все 5 записей на месте");
        let state = js.get_stream(&cfg.name).await.unwrap().get_info().await.unwrap().state;
        assert_eq!(state.first_sequence, 1, "и нумерация не началась заново");
    }

    #[tokio::test]
    async fn a_new_event_type_extends_the_subjects_without_losing_anything() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let old = config(&scope, &["post.>"]);
        scope.track(&old.name);
        ensure_stream(&js, old.clone()).await.unwrap();
        publish(&js, scope.subject("post.liked"), 3).await;
        let newer = config(&scope, &["post.>", "user.followed"]);
        let stream = ensure_stream(&js, newer.clone()).await.expect("запуск новой версии");
        assert_eq!(stream.cached_info().config.subjects, newer.subjects, "темы обновлены");
        publish(&js, scope.subject("user.followed"), 2).await;
        assert_eq!(messages(&js, &old.name).await, 5, "старые записи целы, новая тема пишется в тот же stream");
    }

    #[tokio::test]
    async fn settings_the_broker_will_not_change_are_an_error_and_nothing_is_touched() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let cfg = config(&scope, &["post.>"]);
        scope.track(&cfg.name);
        ensure_stream(&js, cfg.clone()).await.unwrap();
        publish(&js, scope.subject("post.liked"), 4).await;
        let conflicting = Config { retention: RetentionPolicy::WorkQueue, subjects: vec![scope.subject("post.>"), scope.subject("other")], ..cfg.clone() };
        assert!(ensure_stream(&js, conflicting).await.is_err(), "смену retention на очередь заданий брокер отвергает: запуск должен сообщить об ошибке");
        assert_eq!(messages(&js, &cfg.name).await, 4, "при ошибке записи остаются на месте");
    }
}
