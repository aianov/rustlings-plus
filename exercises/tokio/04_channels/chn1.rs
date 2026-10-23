// Сборщик отчётов: несколько рабочих присылают по одному сообщению одному получателю через
// mpsc-канал (multiple producer, single consumer). Сейчас файл не собирается: рабочих много,
// а отправитель в канале один.
// TODO: Почини `collect_reports`, чтобы каждый рабочий смог отправить своё сообщение.
use tokio::sync::mpsc;

async fn collect_reports(workers: usize) -> Vec<String> {
    let (tx, mut rx) = mpsc::channel::<String>(16);
    for id in 0..workers {
        tokio::spawn(async move {
            tx.send(format!("рабочий {id}: готово")).await.unwrap();
        });
    }
    let mut reports = Vec::new();
    for _ in 0..workers {
        reports.push(rx.recv().await.unwrap());
    }
    reports.sort();
    reports
}

#[tokio::main]
async fn main() {
    println!("{:?}", collect_reports(3).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn every_worker_reports() {
        let reports = collect_reports(3).await;
        assert_eq!(
            reports,
            vec!["рабочий 0: готово", "рабочий 1: готово", "рабочий 2: готово"],
            "должны прийти отчёты всех трёх рабочих, по одному от каждого"
        );
    }

    #[tokio::test]
    async fn no_workers_no_reports() {
        assert!(collect_reports(0).await.is_empty(), "без рабочих отчётов нет");
    }

    #[tokio::test]
    async fn many_workers() {
        assert_eq!(collect_reports(8).await.len(), 8, "восемь рабочих, восемь отчётов");
    }
}
