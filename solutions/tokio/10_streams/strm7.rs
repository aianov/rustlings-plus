use futures::stream::{Stream, TryStreamExt};

#[derive(Debug, PartialEq)]
struct DbError(&'static str);

async fn sum_rows<S: Stream<Item = Result<u32, DbError>> + Unpin>(mut rows: S) -> Result<u32, DbError> {
    let mut sum = 0;
    while let Some(n) = rows.try_next().await? {
        sum += n;
    }
    Ok(sum)
}

async fn collect_rows<S: Stream<Item = Result<u32, DbError>>>(rows: S) -> Result<Vec<u32>, DbError> {
    rows.try_collect().await
}

#[tokio::main]
async fn main() {
    let rows = futures::stream::iter(vec![Ok(1), Ok(2)]);
    println!("{:?}", sum_rows(rows).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::{self, StreamExt};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn rows(pulled: &AtomicUsize) -> impl Stream<Item = Result<u32, DbError>> + Unpin + '_ {
        stream::iter(vec![Ok(1), Ok(2), Err(DbError("связь оборвалась")), Ok(4)]).inspect(move |_| {
            pulled.fetch_add(1, Ordering::SeqCst);
        })
    }

    #[tokio::test]
    async fn sum_stops_at_the_first_error() {
        let pulled = AtomicUsize::new(0);
        let got = sum_rows(rows(&pulled)).await;
        assert_eq!(got, Err(DbError("связь оборвалась")), "первая ошибка должна прийти наверх");
        assert_eq!(pulled.load(Ordering::SeqCst), 3, "после ошибки источник трогать нельзя: прочитано должно быть ровно три элемента");
    }

    #[tokio::test]
    async fn collect_stops_at_the_first_error() {
        let pulled = AtomicUsize::new(0);
        let got = collect_rows(rows(&pulled)).await;
        assert_eq!(got, Err(DbError("связь оборвалась")), "ошибка вместо вектора");
        assert_eq!(pulled.load(Ordering::SeqCst), 3, "сбор тоже прекращается на первой ошибке");
    }

    #[tokio::test]
    async fn clean_input() {
        let ok = || stream::iter(vec![Ok::<u32, DbError>(5), Ok(6)]);
        assert_eq!(sum_rows(ok()).await, Ok(11), "сумма без ошибок");
        assert_eq!(collect_rows(ok()).await, Ok(vec![5, 6]), "сбор без ошибок");
    }
}
