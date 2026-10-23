// API отдаёт ленту страницами: на запрос с курсором приходят посты и курсор следующей страницы
// (или его отсутствие, если страницы кончились). Потребителю нужен плоский поток постов, и
// тянуться должно лениво: взял три поста, значит, третья страница не нужна. Если страница
// не загрузилась, потребитель получает ошибку, и поток на этом кончается, а не пробует снова.
// TODO: Допиши `pages` (поток страниц по курсору) и `items` (плоский поток постов).
use futures::stream::{self, Stream, TryStreamExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, PartialEq)]
struct ApiError(String);

struct Page {
    items: Vec<u32>,
    next: Option<usize>,
}

struct Api {
    pages: Vec<Vec<u32>>,
    broken_page: Option<usize>,
    calls: AtomicUsize,
}

impl Api {
    async fn page(&self, cursor: usize) -> Result<Page, ApiError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::task::yield_now().await;
        if self.broken_page == Some(cursor) {
            return Err(ApiError(format!("страница {cursor} недоступна")));
        }
        let next = (cursor + 1 < self.pages.len()).then_some(cursor + 1);
        Ok(Page { items: self.pages[cursor].clone(), next })
    }
}

fn pages(api: Arc<Api>) -> impl Stream<Item = Result<Vec<u32>, ApiError>> {
    let _ = api;
    stream::empty()
}

fn items(api: Arc<Api>) -> impl Stream<Item = Result<u32, ApiError>> {
    pages(api).map_ok(|_| stream::empty::<Result<u32, ApiError>>()).try_flatten()
}

#[tokio::main]
async fn main() {
    use futures::StreamExt;
    let api = Arc::new(Api { pages: vec![vec![1, 2], vec![3]], broken_page: None, calls: AtomicUsize::new(0) });
    println!("{:?}", items(api).collect::<Vec<_>>().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn api(broken_page: Option<usize>) -> Arc<Api> {
        let pages = (0..4u32).map(|p| (0..5).map(|i| p * 10 + i).collect()).collect();
        Arc::new(Api { pages, broken_page, calls: AtomicUsize::new(0) })
    }

    #[tokio::test]
    async fn all_items_in_order() {
        let api = api(None);
        let got: Vec<_> = items(api.clone()).collect().await;
        let expected: Vec<Result<u32, ApiError>> = (0..4u32).flat_map(|p| (0..5).map(move |i| Ok(p * 10 + i))).collect();
        assert_eq!(got, expected, "все посты всех страниц по порядку");
        assert_eq!(api.calls.load(Ordering::SeqCst), 4, "каждая страница запрашивается ровно один раз");
    }

    #[tokio::test]
    async fn pages_are_loaded_lazily() {
        let api = api(None);
        let first: Vec<_> = items(api.clone()).take(3).collect().await;
        assert_eq!(first, vec![Ok(0), Ok(1), Ok(2)], "первые три поста");
        assert_eq!(api.calls.load(Ordering::SeqCst), 1, "трёх постов хватает одной страницы: остальные запрашивать рано");
    }

    #[tokio::test]
    async fn pages_stream_does_not_prefetch() {
        let api = api(None);
        let mut pages = Box::pin(pages(api.clone()));
        assert_eq!(api.calls.load(Ordering::SeqCst), 0, "пока поток не опрашивали, запросов быть не должно");
        pages.next().await.unwrap().unwrap();
        pages.next().await.unwrap().unwrap();
        assert_eq!(api.calls.load(Ordering::SeqCst), 2, "две страницы взяты, два запроса");
    }

    #[tokio::test]
    async fn broken_page_ends_the_stream() {
        let api = api(Some(2));
        let got = tokio::time::timeout(std::time::Duration::from_secs(2), items(api.clone()).collect::<Vec<_>>())
            .await
            .expect("поток не закончился после ошибки страницы");
        assert_eq!(got.len(), 11, "десять постов двух хороших страниц и одна ошибка");
        assert_eq!(got[10], Err(ApiError("страница 2 недоступна".to_string())), "за постами идёт ошибка сломанной страницы");
        assert_eq!(api.calls.load(Ordering::SeqCst), 3, "после ошибки новых запросов нет");
    }
}
