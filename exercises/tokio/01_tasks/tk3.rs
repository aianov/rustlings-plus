// У tokio::spawn жёсткое требование: Future должен быть 'static, то есть не держать ни
// одной ссылки на чужие данные. Задача может прожить дольше функции, которая её запустила,
// и ссылка к тому времени повиснет. Обе функции ниже не собираются.
// TODO: Почини их: слова считаются в задачах, а исходные строки живут не дольше вызова.
use tokio::task::JoinHandle;

fn spawn_word_count(text: &str) -> JoinHandle<usize> {
    tokio::spawn(async move { text.split_whitespace().count() })
}

async fn count_all(texts: &[String]) -> usize {
    let mut handles = Vec::new();
    for text in texts {
        handles.push(tokio::spawn(async move { text.split_whitespace().count() }));
    }
    let mut total = 0;
    for handle in handles {
        total += handle.await.unwrap();
    }
    total
}

#[tokio::main]
async fn main() {
    println!("{}", spawn_word_count("раз два три").await.unwrap());
    println!("{}", count_all(&["a b".to_string(), "c".to_string()]).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn counts_words_of_a_temporary_string() {
        let handle = {
            let local = String::from("один два три четыре");
            spawn_word_count(&local)
        };
        assert_eq!(handle.await.unwrap(), 4, "строка умерла до конца задачи, поэтому задаче нужна своя копия");
    }

    #[tokio::test]
    async fn counts_all_texts() {
        let texts = vec!["а б".to_string(), "в".to_string(), "".to_string(), "г д е".to_string()];
        assert_eq!(count_all(&texts).await, 6, "сумма слов по всем строкам");
    }
}
