// Слова переводят в верхний регистр несколько задач, результат собирается в общий список.
// Автор взял Rc<RefCell<..>>, привычный из однопоточного кода. Но spawn может отправить задачу
// на другой поток, а Rc для этого не годится: счётчик ссылок у него не атомарный.
// TODO: Почини сборку. Читай сообщение компилятора про Send до конца.
use std::cell::RefCell;
use std::rc::Rc;

async fn shout_all(words: Vec<String>) -> Vec<String> {
    let shared = Rc::new(RefCell::new(Vec::new()));
    let mut handles = Vec::new();
    for word in words {
        let shared = Rc::clone(&shared);
        handles.push(tokio::spawn(async move {
            shared.borrow_mut().push(word.to_uppercase());
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    let mut all = shared.borrow().clone();
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
