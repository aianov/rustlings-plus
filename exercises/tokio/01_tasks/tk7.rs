// Паника внутри spawn-нутой задачи не роняет программу: tokio ловит её, а владелец хэндла
// узнаёт о ней из ошибки JoinError. Эту ошибку нужно уметь разбирать: отмена это не паника,
// а у паники есть полезная нагрузка (сообщение).
// TODO: Допиши три функции: достать текст паники, превратить итог задачи в JobError
// и «пробросить» панику дальше, как будто она случилась у нас. Если задачу в propagate
// отменили, паникуй сообщением "задачу отменили".
use std::any::Any;
use tokio::task::JoinHandle;

#[derive(Debug, PartialEq)]
enum JobError {
    Panicked(String),
    Cancelled,
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    todo!()
}

async fn supervise(handle: JoinHandle<u32>) -> Result<u32, JobError> {
    todo!()
}

async fn propagate(handle: JoinHandle<u32>) -> u32 {
    todo!()
}

#[tokio::main]
async fn main() {
    let handle = tokio::spawn(async { 5 });
    println!("{:?}", supervise(handle).await);
    let handle = tokio::spawn(async { 6 });
    println!("{}", propagate(handle).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panicking(code: u32) -> JoinHandle<u32> {
        tokio::spawn(async move {
            assert!(code == 0, "не повезло");
            code
        })
    }

    #[tokio::test]
    async fn success_passes_through() {
        assert_eq!(supervise(tokio::spawn(async { 7 })).await, Ok(7), "успешный результат возвращается как есть");
    }

    #[tokio::test]
    async fn static_panic_message() {
        assert_eq!(
            supervise(panicking(1)).await,
            Err(JobError::Panicked("не повезло".to_string())),
            "текст паники (&str) нужно достать из нагрузки"
        );
    }

    #[tokio::test]
    async fn formatted_panic_message() {
        let handle = tokio::spawn(async {
            let code = 42;
            if code > 0 {
                panic!("код {code}");
            }
            0
        });
        assert_eq!(
            supervise(handle).await,
            Err(JobError::Panicked("код 42".to_string())),
            "паника с форматированием несёт String, а не &str: проверь оба варианта"
        );
    }

    #[tokio::test]
    async fn cancel_is_not_a_panic() {
        let handle: JoinHandle<u32> = tokio::spawn(std::future::pending());
        handle.abort();
        assert_eq!(supervise(handle).await, Err(JobError::Cancelled), "отмена должна быть Cancelled, а не Panicked");
    }

    #[tokio::test]
    async fn propagate_keeps_the_original_panic() {
        let outer = tokio::spawn(propagate(panicking(1)));
        let error = outer.await.unwrap_err();
        assert!(error.is_panic(), "propagate должна паниковать сама");
        assert_eq!(panic_message(error.into_panic()), "не повезло", "паника должна быть та же, а не новая");
    }

    #[tokio::test]
    async fn propagate_panics_on_cancel() {
        let handle: JoinHandle<u32> = tokio::spawn(std::future::pending());
        handle.abort();
        let error = tokio::spawn(propagate(handle)).await.unwrap_err();
        assert!(error.is_panic(), "отмену propagate должна превращать в панику");
        assert_eq!(panic_message(error.into_panic()), "задачу отменили", "текст паники при отмене задан условием");
    }

    #[tokio::test]
    async fn propagate_returns_value() {
        assert_eq!(propagate(tokio::spawn(async { 3 })).await, 3, "без паники возвращается значение");
    }
}
