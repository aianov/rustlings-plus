// Лайки считает актор, но спросить у него текущее число пока нельзя: команда `Like` ответа не
// несёт. Для вопросов у команды `Count` есть поле для ответа, свой одноразовый канал на каждый запрос.
// TODO: Допиши ветку `Count` в акторе и функцию `likes_of`: спросить число лайков поста и дождаться
// ответа. Если актора уже нет, вернуть `None`. Актор не должен падать, если спросивший передумал.
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

enum Cmd {
    Like(u32),
    Count { post: u32, reply: oneshot::Sender<u32> },
}

fn spawn_likes() -> mpsc::Sender<Cmd> {
    let (tx, mut rx) = mpsc::channel(16);
    tokio::spawn(async move {
        let mut likes: HashMap<u32, u32> = HashMap::new();
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Cmd::Like(post) => *likes.entry(post).or_insert(0) += 1,
                Cmd::Count { post, reply } => {
                    todo!()
                }
            }
        }
    });
    tx
}

async fn likes_of(tx: &mpsc::Sender<Cmd>, post: u32) -> Option<u32> {
    todo!()
}

#[tokio::main]
async fn main() {
    let tx = spawn_likes();
    tx.send(Cmd::Like(1)).await.unwrap();
    println!("{:?}", likes_of(&tx, 1).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn answer_includes_all_earlier_likes() {
        let tx = spawn_likes();
        for _ in 0..3 {
            tx.send(Cmd::Like(1)).await.unwrap();
        }
        assert_eq!(likes_of(&tx, 1).await, Some(3), "три лайка отправлены до вопроса, очередь не переставляет сообщения");
        assert_eq!(likes_of(&tx, 2).await, Some(0), "у поста без лайков ответ 0, а не отсутствие ответа");
    }

    #[tokio::test]
    async fn parallel_questions_get_their_own_answers() {
        let tx = spawn_likes();
        for post in 0..20u32 {
            for _ in 0..post {
                tx.send(Cmd::Like(post)).await.unwrap();
            }
        }
        let answers = futures::future::join_all((0..20u32).map(|post| likes_of(&tx, post))).await;
        for (post, answer) in answers.into_iter().enumerate() {
            assert_eq!(answer, Some(post as u32), "на вопрос про пост {post} пришёл чужой ответ или никакого");
        }
    }

    #[tokio::test]
    async fn abandoned_question_does_not_kill_the_actor() {
        let tx = spawn_likes();
        tx.send(Cmd::Like(1)).await.unwrap();
        let (reply, answer) = oneshot::channel();
        drop(answer);
        tx.send(Cmd::Count { post: 1, reply }).await.unwrap();
        assert_eq!(likes_of(&tx, 1).await, Some(1), "спросивший ушёл, не дождавшись; актор обязан пережить это и отвечать дальше");
    }

    #[tokio::test]
    async fn missing_actor_gives_none() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        assert_eq!(likes_of(&tx, 1).await, None, "актора нет: вместо паники ожидается None");
    }
}
