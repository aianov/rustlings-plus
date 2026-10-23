use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use std::io;

fn main() {}

pub type Body = BoxStream<'static, io::Result<Bytes>>;

#[derive(Debug, PartialEq)]
pub struct TooLarge {
    pub limit: u64,
}

pub fn limit(body: Body, max: u64, declared: Option<u64>) -> Result<Body, TooLarge> {
    if declared.is_some_and(|d| d > max) {
        return Err(TooLarge { limit: max });
    }
    let state = (body, 0u64, false);
    let stream = futures::stream::unfold(state, move |(mut body, seen, done)| async move {
        if done {
            return None;
        }
        match body.next().await? {
            Ok(chunk) => {
                let seen = seen + chunk.len() as u64;
                if seen > max {
                    let err = io::Error::new(io::ErrorKind::FileTooLarge, format!("тело больше {max} байт"));
                    Some((Err(err), (body, seen, true)))
                } else {
                    Some((Ok(chunk), (body, seen, false)))
                }
            }
            Err(e) => Some((Err(e), (body, seen, true))),
        }
    });
    Ok(stream.boxed())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn counted(chunks: usize, size: usize) -> (Body, Arc<AtomicUsize>) {
        let polled = Arc::new(AtomicUsize::new(0));
        let seen = polled.clone();
        let body = futures::stream::iter(0..chunks)
            .map(move |_| {
                seen.fetch_add(1, Ordering::SeqCst);
                Ok(Bytes::from(vec![7u8; size]))
            })
            .boxed();
        (body, polled)
    }

    async fn drain(mut body: Body) -> (usize, Option<io::ErrorKind>) {
        let mut bytes = 0;
        while let Some(item) = body.next().await {
            match item {
                Ok(chunk) => bytes += chunk.len(),
                Err(e) => return (bytes, Some(e.kind())),
            }
        }
        (bytes, None)
    }

    #[tokio::test]
    async fn a_body_under_the_limit_passes_unchanged() {
        let (body, _) = counted(5, 1000);
        let (bytes, error) = drain(limit(body, 10_000, Some(5000)).unwrap()).await;
        assert_eq!((bytes, error), (5000, None), "всё тело должно пройти без изменений");
    }

    #[tokio::test]
    async fn exactly_the_limit_is_allowed() {
        let (body, _) = counted(4, 1000);
        let (bytes, error) = drain(limit(body, 4000, None).unwrap()).await;
        assert_eq!((bytes, error), (4000, None), "ровно max байт это ещё можно");
    }

    #[tokio::test]
    async fn the_chunk_that_crosses_the_limit_is_the_error() {
        let (body, _) = counted(10, 1000);
        let (bytes, error) = drain(limit(body, 3500, None).unwrap()).await;
        assert_eq!(error, Some(io::ErrorKind::FileTooLarge), "на куске, который перешагнул предел, должна быть ошибка FileTooLarge");
        assert_eq!(bytes, 3000, "три целых куска прошли, четвёртый (он выводит за 3500) уже не отдаётся");
    }

    #[tokio::test]
    async fn the_source_is_not_read_beyond_the_failure() {
        let (body, polled) = counted(1000, 1000);
        let mut limited = limit(body, 3500, None).unwrap();
        while let Some(item) = limited.next().await {
            if item.is_err() {
                break;
            }
        }
        assert!(polled.load(Ordering::SeqCst) <= 4, "источник прочитан {} раз: считать надо по ходу, а не дочитывать всё тело до конца", polled.load(Ordering::SeqCst));
        assert!(limited.next().await.is_none(), "после ошибки поток заканчивается");
        assert!(polled.load(Ordering::SeqCst) <= 4, "после ошибки источник больше не трогают");
    }

    #[tokio::test]
    async fn a_declared_size_over_the_limit_is_refused_before_reading() {
        let (body, polled) = counted(10, 1000);
        let result = limit(body, 5000, Some(5001));
        assert!(matches!(result, Err(TooLarge { limit: 5000 })), "заявленный размер больше предела отвергается сразу");
        assert_eq!(polled.load(Ordering::SeqCst), 0, "ни одного куска при этом читать не нужно");
    }

    #[tokio::test]
    async fn a_lying_content_length_does_not_help() {
        let (body, _) = counted(50, 1000);
        let (bytes, error) = drain(limit(body, 5000, Some(10)).unwrap()).await;
        assert_eq!(error, Some(io::ErrorKind::FileTooLarge), "заявили 10 байт, прислали 50 000: заявленному нельзя верить");
        assert_eq!(bytes, 5000, "до предела отдано, дальше ошибка");
    }

    #[tokio::test]
    async fn a_source_error_goes_through_and_ends_the_stream() {
        let body = futures::stream::iter(vec![Ok(Bytes::from_static(b"abc")), Err(io::Error::new(io::ErrorKind::ConnectionReset, "обрыв")), Ok(Bytes::from_static(b"zzz"))]).boxed();
        let (bytes, error) = drain(limit(body, 100, None).unwrap()).await;
        assert_eq!((bytes, error), (3, Some(io::ErrorKind::ConnectionReset)), "ошибка источника доходит как есть");
    }
}
