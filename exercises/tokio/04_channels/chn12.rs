// Баннер с заголовком и тегами лежит в watch-канале, а `announce` публикует его по строке
// в журнал, и каждая строка уходит медленно (10 мс). Пока идёт публикация, модератор может
// заменить баннер, но журнал должен получить строки того баннера, который был на момент старта,
// целиком, и модератор не должен ждать конца публикации.
// Сейчас файл не собирается: компилятор не верит, что `announce` можно отдать в spawn.
// TODO: Почини `announce` по смыслу, а не заглушкой.
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

#[derive(Clone, Debug)]
struct Banner {
    title: String,
    tags: Vec<String>,
}

async fn publish_line(log: &Arc<Mutex<Vec<String>>>, line: String) {
    tokio::time::sleep(Duration::from_millis(10)).await;
    log.lock().unwrap().push(line);
}

async fn announce(rx: watch::Receiver<Banner>, log: Arc<Mutex<Vec<String>>>) {
    let banner = rx.borrow();
    publish_line(&log, format!("заголовок: {}", banner.title)).await;
    for tag in &banner.tags {
        publish_line(&log, format!("тег: {tag}")).await;
    }
}

#[tokio::main]
async fn main() {
    let banner = Banner { title: "привет".into(), tags: vec!["раз".into()] };
    let (_tx, rx) = watch::channel(banner);
    let log = Arc::new(Mutex::new(Vec::new()));
    tokio::spawn(announce(rx, Arc::clone(&log))).await.unwrap();
    println!("{:?}", log.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn banner(title: &str, tags: &[&str]) -> Banner {
        Banner { title: title.into(), tags: tags.iter().map(|t| t.to_string()).collect() }
    }

    #[tokio::test(start_paused = true)]
    async fn publishes_a_consistent_snapshot_while_moderator_edits() {
        let (tx, rx) = watch::channel(banner("старый", &["a", "b"]));
        let log = Arc::new(Mutex::new(Vec::new()));
        let publishing = tokio::spawn(announce(rx, Arc::clone(&log)));
        tokio::time::sleep(Duration::from_millis(15)).await;
        let before = tokio::time::Instant::now();
        tx.send(banner("новый", &["x", "y", "z"])).unwrap();
        assert_eq!(before.elapsed(), Duration::ZERO, "модератор не должен ждать конца публикации");
        publishing.await.unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            vec!["заголовок: старый", "тег: a", "тег: b"],
            "в журнале должны быть строки целиком того баннера, что был на старте"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn next_announcement_sees_the_new_banner() {
        let (tx, rx) = watch::channel(banner("старый", &["a"]));
        let log = Arc::new(Mutex::new(Vec::new()));
        tx.send(banner("новый", &["x"])).unwrap();
        tokio::spawn(announce(rx, Arc::clone(&log))).await.unwrap();
        assert_eq!(*log.lock().unwrap(), vec!["заголовок: новый", "тег: x"], "объявление берёт актуальный баннер");
    }
}
