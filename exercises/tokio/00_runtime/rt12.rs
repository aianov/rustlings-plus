// У функции greeting был синхронный помощник load_name, и внутри он сам запускает рантайм.
// Пока greeting вызывали из обычного кода, всё было хорошо. Теперь её зовут из async-кода,
// и tokio паникует: нельзя запускать block_on на потоке, который уже занят рантаймом.
// TODO: Сделай так, чтобы greeting работала из async-кода. Рантайм должен создаваться
// один раз, на самой границе sync и async (run_greetings).
use std::time::Duration;
use tokio::runtime::Runtime;

async fn fetch_name(id: u32) -> String {
    tokio::time::sleep(Duration::from_millis(1)).await;
    format!("user{id}")
}

fn load_name(id: u32) -> String {
    Runtime::new().unwrap().block_on(fetch_name(id))
}

async fn greeting(id: u32) -> String {
    format!("Привет, {}!", load_name(id))
}

fn run_greetings(ids: &[u32]) -> Vec<String> {
    let runtime = Runtime::new().unwrap();
    runtime.block_on(async {
        let mut all = Vec::new();
        for id in ids {
            all.push(greeting(*id).await);
        }
        all
    })
}

fn main() {
    println!("{:?}", run_greetings(&[1, 2]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn greeting_works_inside_async_code() {
        assert_eq!(greeting(7).await, "Привет, user7!", "greeting должна работать, будучи вызванной из async");
    }

    #[test]
    fn boundary_creates_the_runtime_once() {
        assert_eq!(
            run_greetings(&[1, 2, 3]),
            ["Привет, user1!", "Привет, user2!", "Привет, user3!"],
            "run_greetings отдаёт приветствия по порядку id"
        );
    }

    #[test]
    fn empty_input() {
        assert!(run_greetings(&[]).is_empty(), "для пустого списка ответ пуст");
    }
}
