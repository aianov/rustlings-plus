// Ленту собирают из нескольких источников: каждый источник это пачка номеров постов.
// `fan_in` запускает по задаче на источник, все они пишут в один канал, а наружу отдаётся
// получатель. `collect_all` вычитывает получателя до конца. Сколько всего придёт сообщений,
// не знает никто: конец должен определяться тем, что писать больше некому.
// TODO: Допиши `fan_in` и `collect_all`.
use tokio::sync::mpsc::{self, Receiver};

fn fan_in(sources: Vec<Vec<u32>>) -> Receiver<u32> {
    todo!()
}

async fn collect_all(mut rx: Receiver<u32>) -> Vec<u32> {
    todo!()
}

#[tokio::main]
async fn main() {
    let rx = fan_in(vec![vec![1, 2, 3], vec![10, 20]]);
    println!("{:?}", collect_all(rx).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    async fn run(sources: Vec<Vec<u32>>) -> Vec<u32> {
        timeout(Duration::from_secs(1), collect_all(fan_in(sources)))
            .await
            .expect("collect_all не закончился: канал не закрылся, когда все источники отправили своё")
    }

    #[tokio::test(start_paused = true)]
    async fn gathers_everything() {
        let mut all = run(vec![vec![1, 2, 3], vec![10, 20], vec![], vec![100]]).await;
        all.sort();
        assert_eq!(all, vec![1, 2, 3, 10, 20, 100], "должны прийти все номера из всех источников");
    }

    #[tokio::test(start_paused = true)]
    async fn keeps_order_inside_a_source() {
        let all = run(vec![vec![1, 2, 3, 4, 5, 6, 7, 8, 9], vec![50, 60, 70, 80, 90]]).await;
        let first: Vec<u32> = all.iter().copied().filter(|x| *x < 10).collect();
        assert_eq!(first, (1..10).collect::<Vec<_>>(), "порядок внутри одного источника не должен меняться");
    }

    #[tokio::test(start_paused = true)]
    async fn no_sources() {
        assert!(run(vec![]).await.is_empty(), "без источников канал сразу пуст и закрыт");
    }
}
