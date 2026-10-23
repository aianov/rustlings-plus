use std::sync::{Arc, Mutex};

async fn shout_all(words: Vec<String>) -> Vec<String> {
    let shared = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for word in words {
        let shared = Arc::clone(&shared);
        handles.push(tokio::spawn(async move {
            shared.lock().unwrap().push(word.to_uppercase());
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    let mut all = shared.lock().unwrap().clone();
    all.sort();
    all
}

#[tokio::main]
async fn main() {
    println!("{:?}", shout_all(vec!["раз".to_string(), "два".to_string()]).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn uppercases_all() {
        let words = vec!["звезда".to_string(), "небо".to_string(), "луна".to_string()];
        assert_eq!(
            shout_all(words).await,
            ["ЗВЕЗДА", "ЛУНА", "НЕБО"],
            "все слова в верхнем регистре, отсортированные"
        );
    }

    #[tokio::test]
    async fn works_on_a_multi_thread_runtime() {
        let handle = tokio::spawn(shout_all(vec!["a".to_string(), "b".to_string()]));
        assert_eq!(handle.await.unwrap(), ["A", "B"], "сама shout_all тоже должна отправляться через spawn");
    }
}
