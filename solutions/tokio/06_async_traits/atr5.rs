use futures::future::BoxFuture;
use std::time::Duration;
use tokio::time::sleep;

struct MaxLen(usize);

struct NoWords(Vec<&'static str>);

struct Pause(u64);

async fn validate_all(validators: &[Box<dyn Validator>], input: &str) -> Vec<String> {
    let checks = validators.iter().map(|validator| validator.check(input));
    futures::future::join_all(checks).await.into_iter().filter_map(Result::err).collect()
}

trait Validator: Send + Sync {
    fn check<'a>(&'a self, input: &'a str) -> BoxFuture<'a, Result<(), String>>;
}

impl Validator for MaxLen {
    fn check<'a>(&'a self, input: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let len = input.chars().count();
            if len > self.0 { Err(format!("слишком длинно: {len} из {}", self.0)) } else { Ok(()) }
        })
    }
}

impl Validator for NoWords {
    fn check<'a>(&'a self, input: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            match self.0.iter().find(|word| input.contains(**word)) {
                Some(word) => Err(format!("запрещено слово {word}")),
                None => Ok(()),
            }
        })
    }
}

impl Validator for Pause {
    fn check<'a>(&'a self, _input: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            sleep(Duration::from_millis(self.0)).await;
            Ok(())
        })
    }
}

#[tokio::main]
async fn main() {
    let validators: Vec<Box<dyn Validator>> = vec![Box::new(MaxLen(10))];
    println!("{:?}", validate_all(&validators, "привет").await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    fn all() -> Vec<Box<dyn Validator>> {
        vec![Box::new(MaxLen(8)), Box::new(NoWords(vec!["spam", "казино"])), Box::new(Pause(50)), Box::new(Pause(30))]
    }

    #[tokio::test(start_paused = true)]
    async fn clean_text_passes_every_validator() {
        let errors = validate_all(&all(), "всем да").await;
        assert!(errors.is_empty(), "текст из семи символов без запрещённых слов должен пройти все проверки: {errors:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn every_failure_is_reported() {
        let text = String::from("купи казино spam");
        let errors = validate_all(&all(), &text).await;
        assert_eq!(errors.len(), 2, "должны сработать длина и запрещённое слово, получено: {errors:?}");
        assert!(errors[0].contains("слишком длинно"), "первой в списке стоит проверка длины: {errors:?}");
        assert!(errors[1].contains("spam"), "в ошибке должно быть названо найденное запрещённое слово: {errors:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn input_may_be_a_temporary_borrow() {
        let start = Instant::now();
        let errors = {
            let owned = String::from("ok");
            validate_all(&all(), &owned).await
        };
        assert!(errors.is_empty(), "короткий чистый текст: ошибок нет, получено {errors:?}");
        assert_eq!(start.elapsed(), Duration::from_millis(50), "проверки идут вместе: ждём самую долгую (50 мс)");
    }
}
