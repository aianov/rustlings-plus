// Отчёт собирается из строк, которые пишут несколько задач-писателей. Когда все писатели
// закончили, `collect` должен отдать собранные строки. Но на практике `collect` висит и не
// возвращается никогда, хотя писать уже некому.
// TODO: Почини `Report` так, чтобы `collect` возвращался, а `writer` по-прежнему можно было
// вызывать сколько угодно раз.
use tokio::sync::mpsc;

struct Report {
    tx: mpsc::Sender<String>,
    rx: mpsc::Receiver<String>,
}

impl Report {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel(8);
        Report { tx, rx }
    }

    fn writer(&self) -> mpsc::Sender<String> {
        self.tx.clone()
    }

    async fn collect(mut self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Some(line) = self.rx.recv().await {
            lines.push(line);
        }
        lines.sort();
        lines
    }
}

#[tokio::main]
async fn main() {
    let report = Report::new();
    let w = report.writer();
    tokio::spawn(async move { w.send("раз".to_string()).await.unwrap() });
    println!("{:?}", report.collect().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn collects_from_all_writers() {
        let report = Report::new();
        for i in 0..3 {
            let w = report.writer();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(10 * i)).await;
                w.send(format!("писатель {i}")).await.unwrap();
            });
        }
        let lines = timeout(Duration::from_secs(1), report.collect())
            .await
            .expect("collect завис: все писатели ушли, а канал не закрылся. Кто-то ещё держит Sender");
        assert_eq!(lines, vec!["писатель 0", "писатель 1", "писатель 2"], "должны прийти строки всех писателей");
    }

    #[tokio::test(start_paused = true)]
    async fn nobody_wrote() {
        let report = Report::new();
        let lines = timeout(Duration::from_secs(1), report.collect())
            .await
            .expect("collect завис на пустом отчёте: писателей нет, значит, и ждать нечего");
        assert!(lines.is_empty(), "никто ничего не написал");
    }

    #[tokio::test(start_paused = true)]
    async fn writer_can_be_requested_again() {
        let report = Report::new();
        let first = report.writer();
        let second = report.writer();
        first.send("a".to_string()).await.unwrap();
        second.send("b".to_string()).await.unwrap();
        drop(first);
        drop(second);
        let lines = timeout(Duration::from_secs(1), report.collect()).await.expect("collect завис");
        assert_eq!(lines, vec!["a", "b"], "несколько писателей подряд должны работать");
    }
}
