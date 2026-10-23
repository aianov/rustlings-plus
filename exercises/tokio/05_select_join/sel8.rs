// Сводка активности: лайки приходят по одному каналу, комментарии по другому. Функция суммирует
// и тех, и других, пока приходят хоть какие-то, и возвращает итог, когда закрылись оба канала.
// Сейчас она не возвращает итог, а падает с паникой, когда источники заканчиваются. Тесты
// воспроизводят это, в том числе когда один канал закрылся раньше другого.
// TODO: Почини `tally`.
use tokio::sync::mpsc;

async fn tally(mut likes: mpsc::Receiver<u32>, mut comments: mpsc::Receiver<String>) -> (u32, usize) {
    let mut total_likes = 0;
    let mut total_comments = 0;
    loop {
        tokio::select! {
            Some(count) = likes.recv() => total_likes += count,
            Some(_) = comments.recv() => total_comments += 1,
        }
    }
}

#[tokio::main]
async fn main() {
    let (likes_tx, likes) = mpsc::channel(4);
    let (comments_tx, comments) = mpsc::channel(4);
    likes_tx.send(2).await.unwrap();
    comments_tx.send("классно".to_string()).await.unwrap();
    drop((likes_tx, comments_tx));
    println!("{:?}", tally(likes, comments).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(start_paused = true)]
    async fn both_channels_close_at_once() {
        let (likes_tx, likes) = mpsc::channel(8);
        let (comments_tx, comments) = mpsc::channel(8);
        likes_tx.send(2).await.unwrap();
        likes_tx.send(5).await.unwrap();
        comments_tx.send("a".to_string()).await.unwrap();
        drop((likes_tx, comments_tx));
        assert_eq!(tally(likes, comments).await, (7, 1), "сумма лайков 7, комментарий один");
    }

    #[tokio::test(start_paused = true)]
    async fn one_channel_outlives_the_other() {
        let (likes_tx, likes) = mpsc::channel(8);
        let (comments_tx, comments) = mpsc::channel(8);
        likes_tx.send(1).await.unwrap();
        drop(likes_tx);
        let slow = tokio::spawn(async move {
            for text in ["a", "b", "c"] {
                sleep(Duration::from_millis(100)).await;
                comments_tx.send(text.to_string()).await.unwrap();
            }
        });
        let result = tally(likes, comments).await;
        slow.await.unwrap();
        assert_eq!(result, (1, 3), "лайки закончились, но комментарии ещё идут: сводка должна дождаться и их");
    }

    #[tokio::test(start_paused = true)]
    async fn silence_is_not_the_end() {
        let (likes_tx, likes) = mpsc::channel(8);
        let (comments_tx, comments) = mpsc::channel::<String>(8);
        let late = tokio::spawn(async move {
            sleep(Duration::from_secs(60)).await;
            likes_tx.send(9).await.unwrap();
            drop(comments_tx);
        });
        let result = tally(likes, comments).await;
        late.await.unwrap();
        assert_eq!(result, (9, 0), "пока один канал открыт, тишина в нём не конец: ждём до закрытия");
    }
}
