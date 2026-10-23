use std::rc::Rc;
use std::time::Duration;
use tokio::task::JoinHandle;

async fn pause() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

fn load_numbers(numbers: Vec<u32>) -> Rc<Vec<u32>> {
    Rc::new(numbers)
}

fn spawn_report(numbers: Vec<u32>) -> JoinHandle<u32> {
    tokio::spawn(async move {
        let (sum, count) = {
            let shared = load_numbers(numbers);
            (shared.iter().sum::<u32>(), shared.len() as u32)
        };
        pause().await;
        sum + count
    })
}

#[tokio::main]
async fn main() {
    println!("{}", spawn_report(vec![1, 2, 3]).await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn report_is_sum_plus_count() {
        assert_eq!(spawn_report(vec![1, 2, 3]).await.unwrap(), 9, "сумма 6 плюс количество 3");
    }

    #[tokio::test]
    async fn empty_report() {
        assert_eq!(spawn_report(vec![]).await.unwrap(), 0, "для пустого списка 0");
    }
}
