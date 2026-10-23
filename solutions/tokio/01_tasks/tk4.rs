use std::sync::Arc;

async fn parallel_sum(data: Vec<u64>, parts: usize) -> u64 {
    let data = Arc::new(data);
    let mut handles = Vec::new();
    for part in 0..parts {
        let data = Arc::clone(&data);
        handles.push(tokio::spawn(async move {
            data.iter().skip(part).step_by(parts).sum::<u64>()
        }));
    }
    let mut total = 0;
    for handle in handles {
        total += handle.await.unwrap();
    }
    total
}

#[tokio::main]
async fn main() {
    println!("{}", parallel_sum((1..=100).collect(), 4).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sums_everything_once() {
        assert_eq!(parallel_sum((1..=100).collect(), 4).await, 5050, "каждый элемент входит ровно в одну долю");
    }

    #[tokio::test]
    async fn one_part() {
        assert_eq!(parallel_sum(vec![5, 6, 7], 1).await, 18, "одна доля берёт всё");
    }

    #[tokio::test]
    async fn more_parts_than_items() {
        assert_eq!(parallel_sum(vec![1, 2, 3], 7).await, 6, "лишние доли просто пусты");
    }

    #[tokio::test]
    async fn empty() {
        assert_eq!(parallel_sum(vec![], 3).await, 0, "пустой вход даёт 0");
    }
}
