use std::time::Duration;
use tokio::time::sleep;

struct Store {
    name_ms: u64,
    followers_ms: u64,
    post_ms: u64,
}

impl Store {
    async fn name(&self, id: u32) -> String {
        sleep(Duration::from_millis(self.name_ms)).await;
        format!("user{id}")
    }

    async fn followers(&self, id: u32) -> u32 {
        sleep(Duration::from_millis(self.followers_ms)).await;
        id * 10
    }

    async fn last_post(&self, id: u32) -> Option<String> {
        sleep(Duration::from_millis(self.post_ms)).await;
        (id % 2 == 1).then(|| format!("post of {id}"))
    }
}

#[derive(Debug, PartialEq)]
struct Card {
    name: String,
    followers: u32,
    last_post: Option<String>,
}

async fn load_card(store: &Store, id: u32) -> Card {
    let (name, followers, last_post) = tokio::join!(store.name(id), store.followers(id), store.last_post(id));
    Card { name, followers, last_post }
}

#[tokio::main]
async fn main() {
    let store = Store { name_ms: 100, followers_ms: 60, post_ms: 40 };
    println!("{:?}", load_card(&store, 7).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn card_is_complete() {
        let store = Store { name_ms: 100, followers_ms: 60, post_ms: 40 };
        let card = load_card(&store, 7).await;
        assert_eq!(
            card,
            Card { name: "user7".into(), followers: 70, last_post: Some("post of 7".into()) },
            "карточка собрана неверно: каждое поле берётся из своего запроса"
        );
        assert_eq!(load_card(&store, 8).await.last_post, None, "у чётных id постов нет");
    }

    #[tokio::test(start_paused = true)]
    async fn waits_only_for_the_slowest() {
        let store = Store { name_ms: 100, followers_ms: 60, post_ms: 40 };
        let start = Instant::now();
        load_card(&store, 7).await;
        assert_eq!(
            start.elapsed(),
            Duration::from_millis(100),
            "страница ждёт сумму времён (200 мс), а нужен самый долгий запрос (100 мс)"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn slowest_can_be_any_of_them() {
        let store = Store { name_ms: 10, followers_ms: 20, post_ms: 300 };
        let start = Instant::now();
        load_card(&store, 1).await;
        assert_eq!(start.elapsed(), Duration::from_millis(300), "время страницы равно времени самого долгого запроса, каким бы он ни был");
    }
}
