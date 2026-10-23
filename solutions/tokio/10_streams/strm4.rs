use futures::stream::{self, StreamExt};
use std::time::Duration;
use tokio::time::sleep;

async fn title_of(id: u32) -> String {
    sleep(Duration::from_millis(5)).await;
    "x".repeat((id % 7) as usize + 1)
}

async fn longest_title(ids: Vec<u32>) -> Option<String> {
    let titles = stream::iter(ids).then(|id| async move { title_of(id).await });
    let mut titles = std::pin::pin!(titles);
    let mut best: Option<String> = None;
    while let Some(title) = titles.next().await {
        if best.as_ref().is_none_or(|b| title.len() > b.len()) {
            best = Some(title);
        }
    }
    best
}

#[tokio::main]
async fn main() {
    println!("{:?}", longest_title(vec![1, 2, 3]).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn longest_wins() {
        let got = longest_title(vec![1, 5, 3, 6]).await;
        assert_eq!(got, Some("xxxxxxx".to_string()), "id 6 даёт название из семи символов, самое длинное");
    }

    #[tokio::test(start_paused = true)]
    async fn first_of_equal_length_wins() {
        let got = longest_title(vec![2, 9, 2]).await;
        assert_eq!(got, Some("xxx".to_string()), "id 2 и 9 дают по три символа: берём первое");
    }

    #[tokio::test(start_paused = true)]
    async fn empty() {
        assert_eq!(longest_title(vec![]).await, None, "из пустого списка ничего не выйдет");
    }
}
