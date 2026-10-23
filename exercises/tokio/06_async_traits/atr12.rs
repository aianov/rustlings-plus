// Отчёты запускаются десятками тысяч задач сразу, и память съедена почти вся. Причина в размере
// future: задача tokio хранит целиком всё, что живёт через `.await`. Отчёт собирает таблицу в
// массиве на 16 КиБ, пока ждёт ответа внешнего сервиса, и вся сотня тысяч задач носит этот массив с
// собой. Результат отчёта (контрольная сумма) менять нельзя.
// TODO: Сделай так, чтобы future отчёта весил меньше килобайта, а `all_reports` (три отчёта вместе)
// меньше двух килобайт.
use std::time::Duration;
use tokio::time::sleep;

fn checksum(buffer: &[u8]) -> u32 {
    buffer.iter().map(|byte| u32::from(*byte)).sum()
}

async fn make_report(rows: u32) -> u32 {
    let mut buffer = [0u8; 16 * 1024];
    for (index, byte) in buffer.iter_mut().enumerate() {
        *byte = ((index as u32 + rows) % 251) as u8;
    }
    sleep(Duration::from_millis(5)).await;
    checksum(&buffer)
}

async fn all_reports() -> u32 {
    let (a, b, c) = tokio::join!(make_report(1), make_report(2), make_report(3));
    a + b + c
}

#[tokio::main]
async fn main() {
    println!("{}", all_reports().await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected(rows: u32) -> u32 {
        (0..16 * 1024u32).map(|index| (index + rows) % 251).sum()
    }

    #[tokio::test]
    async fn checksums_are_unchanged() {
        assert_eq!(make_report(1).await, expected(1), "контрольная сумма отчёта изменилась: нужны те же 16384 байта данных");
        assert_eq!(make_report(7).await, expected(7), "контрольная сумма зависит от числа строк");
        assert_eq!(all_reports().await, expected(1) + expected(2) + expected(3), "сумма трёх отчётов");
    }

    #[test]
    fn one_report_future_is_small() {
        let size = std::mem::size_of_val(&make_report(1));
        assert!(size < 1024, "future одного отчёта занимает {size} байт: массив живёт через .await и лежит внутри него");
    }

    #[test]
    fn three_reports_together_stay_small() {
        let size = std::mem::size_of_val(&all_reports());
        assert!(size < 2048, "future трёх отчётов занимает {size} байт: каждый отчёт приносит свой массив");
    }
}
