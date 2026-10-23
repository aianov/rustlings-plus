// Порт рассылки: `send` отправляет одно письмо, `send_many` (готовый метод трейта) рассылает список.
// Службе нужно запускать рассылки отдельными задачами и не ждать их, поэтому `spawn_welcome` и
// `spawn_digest` отдают `JoinHandle`. Но это не собирается: порт не обещает, что future его методов
// можно отправлять между потоками. Реализации порта, которые уже есть, менять нельзя.
// TODO: Почини трейт `Mailer`.
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

#[derive(Debug, PartialEq)]
enum MailError {
    Rejected(u32),
}

struct Outbox {
    sent: Mutex<Vec<(u32, String)>>,
    reject: Option<u32>,
}

impl Outbox {
    fn new(reject: Option<u32>) -> Outbox {
        Outbox { sent: Mutex::new(Vec::new()), reject }
    }
}

impl Mailer for Outbox {
    async fn send(&self, to: u32, text: &str) -> Result<(), MailError> {
        if self.reject == Some(to) {
            return Err(MailError::Rejected(to));
        }
        self.sent.lock().unwrap().push((to, text.to_string()));
        Ok(())
    }
}

fn spawn_welcome<M>(mailer: Arc<M>, to: u32, name: &str) -> JoinHandle<Result<(), MailError>>
where
    M: Mailer + Send + Sync + 'static,
{
    let text = format!("Привет, {name}!");
    tokio::spawn(async move { mailer.send(to, &text).await })
}

fn spawn_digest<M>(mailer: Arc<M>, to: Vec<u32>, text: String) -> JoinHandle<usize>
where
    M: Mailer + Send + Sync + 'static,
{
    tokio::spawn(async move { mailer.send_many(&to, &text).await })
}

trait Mailer {
    async fn send(&self, to: u32, text: &str) -> Result<(), MailError>;

    async fn send_many(&self, to: &[u32], text: &str) -> usize {
        let mut delivered = 0;
        for user in to {
            if self.send(*user, text).await.is_ok() {
                delivered += 1;
            }
        }
        delivered
    }
}

#[tokio::main]
async fn main() {
    let outbox = Arc::new(Outbox::new(None));
    println!("{:?}", spawn_welcome(Arc::clone(&outbox), 1, "Аня").await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn welcome_is_sent_from_a_task() {
        let outbox = Arc::new(Outbox::new(None));
        let result = spawn_welcome(Arc::clone(&outbox), 7, "Аня").await.unwrap();
        assert_eq!(result, Ok(()), "письмо должно уйти");
        assert_eq!(*outbox.sent.lock().unwrap(), vec![(7, "Привет, Аня!".to_string())], "текст приветствия собирается из имени");
    }

    #[tokio::test]
    async fn rejected_mail_is_an_error() {
        let outbox = Arc::new(Outbox::new(Some(7)));
        let result = spawn_welcome(outbox, 7, "Аня").await.unwrap();
        assert_eq!(result, Err(MailError::Rejected(7)), "отказ порта должен дойти до вызывающего");
    }

    #[tokio::test]
    async fn digest_counts_delivered() {
        let outbox = Arc::new(Outbox::new(Some(2)));
        let delivered = spawn_digest(Arc::clone(&outbox), vec![1, 2, 3], "новости".to_string()).await.unwrap();
        assert_eq!(delivered, 2, "второму письмо отклонено, остальным ушло");
        let sent = outbox.sent.lock().unwrap().clone();
        assert_eq!(sent.iter().map(|(to, _)| *to).collect::<Vec<_>>(), vec![1, 3], "доставлено первому и третьему");
    }

    struct Counting(AtomicUsize);

    impl Mailer for Counting {
        async fn send(&self, _to: u32, _text: &str) -> Result<(), MailError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn plain_async_fn_impls_and_default_method_still_work() {
        let counting = Arc::new(Counting(AtomicUsize::new(0)));
        let delivered = spawn_digest(Arc::clone(&counting), vec![1, 2, 3, 4], "x".to_string()).await.unwrap();
        assert_eq!(delivered, 4, "готовый send_many должен работать для любой реализации");
        assert_eq!(counting.0.load(Ordering::SeqCst), 4, "send вызван четыре раза");
    }
}
