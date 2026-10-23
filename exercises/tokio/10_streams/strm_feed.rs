// Лента соцсети как поток. Порт `FeedPort` отдаёт посты страницами по курсору и имена авторов
// по номеру. Требования:
//  1. `feed` выдаёт `FeedItem` по порядку страниц и постов. Страницы грузятся лениво: пока
//     потребитель не просит, следующая не запрашивается. Посты скрытых авторов и посты с пустым
//     (из одних пробелов) текстом отбрасываются до обращения к порту за автором.
//  2. Имена авторов подтягиваются параллельно, не больше `LOOKUPS` одновременно, а порядок
//     выдачи сохраняется. Сбой при поиске автора превращает в `Err(FeedError::Author(id))`
//     только этот элемент, поток идёт дальше.
//  3. Сбой страницы выдаётся как `Err(FeedError::Page(..))` после всех предыдущих элементов,
//     и на этом поток кончается: следующих страниц не запрашивают.
//  4. `digest` режет результаты ленты на пачки по `size`; пачка с ошибкой целиком становится
//     `Err` (первой ошибкой), последняя пачка может быть короче.
//  5. `with_live` вливает в ленту живые элементы из канала: поток выдаёт `Entry::History` и
//     `Entry::Live` в порядке прихода и кончается, когда кончились оба источника. Ошибки истории
//     проходят как есть.
use futures::stream::{self, Stream, StreamExt, TryStreamExt};
use std::collections::HashSet;
use std::future::ready;
use std::sync::Arc;
use tokio::sync::mpsc;

const LOOKUPS: usize = 4;

#[derive(Debug, Clone, PartialEq)]
struct Post {
    id: u64,
    author: u32,
    text: String,
}

struct Page {
    posts: Vec<Post>,
    next: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
struct FeedItem {
    post: u64,
    author: String,
    text: String,
}

#[derive(Debug, Clone, PartialEq)]
enum FeedError {
    Page(String),
    Author(u32),
}

#[derive(Debug, Clone, PartialEq)]
enum Entry {
    History(FeedItem),
    Live(FeedItem),
}

trait FeedPort: Send + Sync + 'static {
    async fn page(&self, cursor: Option<u64>) -> Result<Page, String>;
    async fn author(&self, id: u32) -> Result<String, String>;
}

fn rx_stream<T>(rx: mpsc::Receiver<T>) -> impl Stream<Item = T> {
    stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) })
}

fn feed<P: FeedPort>(port: Arc<P>, hidden: HashSet<u32>) -> impl Stream<Item = Result<FeedItem, FeedError>> {
    let _ = (port, hidden, LOOKUPS, ready(()), stream::empty::<u32>().map(|x| x));
    stream::empty()
}

fn digest<S>(items: S, size: usize) -> impl Stream<Item = Result<Vec<FeedItem>, FeedError>>
where
    S: Stream<Item = Result<FeedItem, FeedError>>,
{
    let _ = (items, size);
    stream::empty()
}

fn with_live<S>(history: S, live: mpsc::Receiver<FeedItem>) -> impl Stream<Item = Result<Entry, FeedError>>
where
    S: Stream<Item = Result<FeedItem, FeedError>>,
{
    let _ = (history.map_ok(Entry::History), rx_stream(live));
    stream::empty()
}

#[tokio::main]
async fn main() {
    println!("лента собирается в тестах");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::{sleep, Instant};

    struct Fake {
        pages: Vec<Vec<Post>>,
        broken_page: Option<usize>,
        unknown_author: Option<u32>,
        page_calls: AtomicUsize,
        author_calls: AtomicUsize,
        active: AtomicUsize,
        peak: AtomicUsize,
    }

    impl Fake {
        fn new(pages: Vec<Vec<Post>>) -> Fake {
            Fake {
                pages,
                broken_page: None,
                unknown_author: None,
                page_calls: AtomicUsize::new(0),
                author_calls: AtomicUsize::new(0),
                active: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
            }
        }
    }

    impl FeedPort for Fake {
        async fn page(&self, cursor: Option<u64>) -> Result<Page, String> {
            let index = cursor.unwrap_or(0) as usize;
            self.page_calls.fetch_add(1, Ordering::SeqCst);
            sleep(Duration::from_millis(5)).await;
            if self.broken_page == Some(index) {
                return Err(format!("страница {index} недоступна"));
            }
            let next = (index + 1 < self.pages.len()).then_some(index as u64 + 1);
            Ok(Page { posts: self.pages[index].clone(), next })
        }

        async fn author(&self, id: u32) -> Result<String, String> {
            self.author_calls.fetch_add(1, Ordering::SeqCst);
            let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            let ms = match id {
                1 => 40,
                2 => 10,
                _ => 20,
            };
            sleep(Duration::from_millis(ms)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            if self.unknown_author == Some(id) {
                return Err(format!("автора {id} нет"));
            }
            Ok(format!("user{id}"))
        }
    }

    fn post(id: u64, author: u32, text: &str) -> Post {
        Post { id, author, text: text.to_string() }
    }

    fn plain_pages(pages: u64, per_page: u64) -> Vec<Vec<Post>> {
        (0..pages)
            .map(|p| (0..per_page).map(|i| post(p * per_page + i, (i % 3) as u32 + 1, &format!("post {}", p * per_page + i))).collect())
            .collect()
    }

    fn item(post: u64, author: &str, text: &str) -> FeedItem {
        FeedItem { post, author: author.to_string(), text: text.to_string() }
    }

    #[tokio::test(start_paused = true)]
    async fn hidden_and_blank_posts_are_dropped_before_lookup() {
        let pages = vec![
            vec![post(1, 1, "a"), post(2, 3, "hidden author"), post(3, 2, "   "), post(4, 2, "b")],
            vec![post(5, 3, "hidden again"), post(6, 1, "c")],
        ];
        let port = Arc::new(Fake::new(pages));
        let got: Vec<_> = feed(port.clone(), HashSet::from([3])).collect().await;
        let expected = vec![Ok(item(1, "user1", "a")), Ok(item(4, "user2", "b")), Ok(item(6, "user1", "c"))];
        assert_eq!(got, expected, "скрытый автор и пустой текст отбрасываются, остальное идёт по порядку с именами");
        assert_eq!(port.author_calls.load(Ordering::SeqCst), 3, "за автором нужно ходить только для оставшихся постов");
    }

    #[tokio::test(start_paused = true)]
    async fn lookups_are_parallel_limited_and_ordered() {
        let port = Arc::new(Fake::new(plain_pages(5, 4)));
        let started = Instant::now();
        let got: Vec<_> = feed(port.clone(), HashSet::new()).collect().await;
        let ids: Vec<u64> = got.iter().map(|r| r.as_ref().unwrap().post).collect();
        assert_eq!(ids, (0..20).collect::<Vec<_>>(), "порядок постов должен сохраниться, хотя авторы отвечают с разной скоростью");
        assert_eq!(port.peak.load(Ordering::SeqCst), LOOKUPS, "поиски авторов идут параллельно, ровно {LOOKUPS} одновременно");
        assert!(started.elapsed() < Duration::from_millis(250), "двадцать поисков должны идти по {LOOKUPS} разом, прошло {:?}", started.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn pages_are_pulled_lazily() {
        let port = Arc::new(Fake::new(plain_pages(5, 4)));
        let first: Vec<_> = feed(port.clone(), HashSet::new()).take(2).collect().await;
        assert_eq!(first.len(), 2, "просили два элемента");
        assert!(port.page_calls.load(Ordering::SeqCst) <= 2, "двух элементов хватает первых страниц: запрошено {}", port.page_calls.load(Ordering::SeqCst));
        assert!(port.author_calls.load(Ordering::SeqCst) <= 6, "и поиски авторов не должны убегать далеко вперёд");
    }

    #[tokio::test(start_paused = true)]
    async fn broken_page_ends_the_feed() {
        let mut fake = Fake::new(plain_pages(5, 2));
        fake.broken_page = Some(2);
        let port = Arc::new(fake);
        let got: Vec<_> = feed(port.clone(), HashSet::new()).collect().await;
        assert_eq!(got.len(), 5, "четыре поста двух хороших страниц и одна ошибка");
        assert!(got[..4].iter().all(|r| r.is_ok()), "посты до сломанной страницы приходят как обычно");
        assert_eq!(got[4], Err(FeedError::Page("страница 2 недоступна".to_string())), "после них ошибка страницы");
        assert_eq!(port.page_calls.load(Ordering::SeqCst), 3, "после ошибки новых страниц не запрашивают");
    }

    #[tokio::test(start_paused = true)]
    async fn unknown_author_spoils_only_its_own_item() {
        let mut fake = Fake::new(plain_pages(1, 4));
        fake.unknown_author = Some(2);
        let got: Vec<_> = feed(Arc::new(fake), HashSet::new()).collect().await;
        assert_eq!(got.len(), 4, "поток идёт дальше после сбоя поиска автора");
        assert_eq!(got[1], Err(FeedError::Author(2)), "сбой только у поста с автором 2");
        assert!(got[0].is_ok() && got[2].is_ok() && got[3].is_ok(), "остальные посты в порядке");
    }

    #[tokio::test(start_paused = true)]
    async fn digest_cuts_batches() {
        let port = Arc::new(Fake::new(plain_pages(5, 5)));
        let batches: Vec<_> = digest(feed(port, HashSet::new()), 10).collect().await;
        let sizes: Vec<usize> = batches.iter().map(|b| b.as_ref().unwrap().len()).collect();
        assert_eq!(sizes, vec![10, 10, 5], "25 постов по 10 в пачке, в последней остаток");
    }

    #[tokio::test(start_paused = true)]
    async fn digest_batch_with_error_is_an_error() {
        let mut fake = Fake::new(plain_pages(1, 6));
        fake.unknown_author = Some(3);
        let batches: Vec<_> = digest(feed(Arc::new(fake), HashSet::new()), 3).collect().await;
        assert_eq!(batches.len(), 2, "шесть элементов, две пачки");
        assert_eq!(batches[0].as_ref().unwrap_err(), &FeedError::Author(3), "в первой пачке есть пост автора 3: вся пачка ошибка");
        assert!(batches[1].is_err(), "во второй тоже пост автора 3");
    }

    #[tokio::test(start_paused = true)]
    async fn live_events_are_merged_in_arrival_order() {
        let port = Arc::new(Fake::new(vec![vec![post(1, 2, "h1"), post(2, 2, "h2")]]));
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            sleep(Duration::from_millis(8)).await;
            tx.send(item(100, "live", "early")).await.unwrap();
            sleep(Duration::from_millis(100)).await;
            tx.send(item(101, "live", "late")).await.unwrap();
        });
        let got: Vec<_> = with_live(feed(port, HashSet::new()), rx).collect().await;
        let kinds: Vec<(bool, u64)> = got
            .iter()
            .map(|r| match r.as_ref().unwrap() {
                Entry::History(i) => (false, i.post),
                Entry::Live(i) => (true, i.post),
            })
            .collect();
        assert_eq!(kinds, vec![(true, 100), (false, 1), (false, 2), (true, 101)], "живой элемент пришёл раньше истории, другой позже; порядок прихода");
    }

    #[tokio::test(start_paused = true)]
    async fn live_ends_when_both_sources_end() {
        let port = Arc::new(Fake::new(vec![vec![post(1, 2, "h1")]]));
        let (tx, rx) = mpsc::channel(8);
        drop(tx);
        let got: Vec<_> = tokio::time::timeout(Duration::from_secs(5), with_live(feed(port, HashSet::new()), rx).collect::<Vec<_>>())
            .await
            .expect("слитый поток не закончился");
        assert_eq!(got.len(), 1, "только история, живых нет");
    }
}
