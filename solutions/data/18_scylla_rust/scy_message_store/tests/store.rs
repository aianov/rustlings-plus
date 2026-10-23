use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chatstore::{ChatSummary, MessageStore, StoreError};
use scylla::{Session, SessionBuilder};

struct TestKeyspace {
    session: Arc<Session>,
    name: String,
}

impl TestKeyspace {
    async fn new() -> TestKeyspace {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let uri = std::env::var("RUSTLINGS_SCYLLA_URI").unwrap_or_else(|_| "127.0.0.1:59042".to_string());
        let session = SessionBuilder::new()
            .known_node(&uri)
            .build()
            .await
            .unwrap_or_else(|e| panic!("Scylla курса не отвечает по адресу {uri}: {e}. Запусти `docker compose up -d`."));
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let name = format!("store_{}_{nanos:x}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed));
        session
            .query(format!("CREATE KEYSPACE {name} WITH replication = {{'class': 'SimpleStrategy', 'replication_factor': 1}}"), ())
            .await
            .expect("создание keyspace теста");
        TestKeyspace { session: Arc::new(session), name }
    }

    async fn store(&self) -> MessageStore {
        MessageStore::new(Arc::clone(&self.session), &self.name).await.expect("создание хранилища")
    }
}

impl Drop for TestKeyspace {
    fn drop(&mut self) {
        let session = Arc::clone(&self.session);
        let name = self.name.clone();
        let _ = std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                rt.block_on(async {
                    let _ = session.query(format!("DROP KEYSPACE IF EXISTS {name}"), ()).await;
                });
            }
        })
        .join();
    }
}

#[tokio::test]
async fn history_is_newest_first_and_keeps_same_millisecond() {
    let ks = TestKeyspace::new().await;
    let store = ks.store().await;
    store.send(1, 7, "первое", &[7, 9], 100).await.expect("отправка");
    let a = store.send(1, 9, "одновременно а", &[7, 9], 200).await.expect("отправка");
    let b = store.send(1, 7, "одновременно б", &[7, 9], 200).await.expect("отправка");
    store.send(2, 7, "другой чат", &[7], 150).await.expect("отправка");
    assert_ne!(a, b, "у сообщений разные id");
    let page = store.history(1, None, 10).await.expect("история");
    let bodies: Vec<&str> = page.items.iter().map(|m| m.body.as_str()).collect();
    assert_eq!(bodies.len(), 3, "в чате 1 три сообщения, два в одну миллисекунду не затёрли друг друга: {bodies:?}");
    assert_eq!(bodies[2], "первое", "старое сообщение последним: {bodies:?}");
    assert!(page.items[..2].iter().all(|m| m.sent_at == 200), "сначала свежие");
    assert!(page.next.is_none(), "всё влезло в одну страницу, курсора нет");
}

#[tokio::test]
async fn history_pages_with_a_cursor() {
    let ks = TestKeyspace::new().await;
    let store = ks.store().await;
    for i in 1..=25 {
        store.send(3, 7, &format!("сообщение {i}"), &[7], i).await.expect("отправка");
    }
    let mut cursor: Option<String> = None;
    let mut sizes = Vec::new();
    let mut seen = Vec::new();
    loop {
        let page = store.history(3, cursor.as_deref(), 10).await.expect("страница");
        sizes.push(page.items.len());
        seen.extend(page.items.into_iter().map(|m| m.sent_at));
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
        assert!(sizes.len() < 10, "страницы не кончаются");
    }
    assert_eq!(sizes, [10, 10, 5], "25 сообщений это страницы 10, 10 и 5");
    assert_eq!(seen, (1..=25).rev().collect::<Vec<i64>>(), "без повторов и пропусков, свежие первыми");
    assert_eq!(store.history(3, Some("не курсор"), 10).await, Err(StoreError::BadCursor), "испорченный курсор это BadCursor");
}

#[tokio::test]
async fn chats_are_ordered_by_activity() {
    let ks = TestKeyspace::new().await;
    let store = ks.store().await;
    store.send(1, 7, "a", &[7, 9], 100).await.expect("отправка");
    store.send(2, 9, "b", &[7, 9], 200).await.expect("отправка");
    store.send(1, 9, "c", &[7, 9], 300).await.expect("отправка");
    store.send(3, 12, "d", &[12], 400).await.expect("отправка");
    store.send(2, 9, "старое опоздало", &[7, 9], 150).await.expect("отправка");
    assert_eq!(
        store.chats(7).await.expect("чаты"),
        [ChatSummary { chat: 1, last_at: 300 }, ChatSummary { chat: 2, last_at: 200 }],
        "у 7 каждый чат один раз, свежий наверху; опоздавшее старое сообщение не двигает чат назад"
    );
    assert_eq!(store.chats(12).await.expect("чаты"), [ChatSummary { chat: 3, last_at: 400 }]);
    assert!(store.chats(99).await.expect("чаты").is_empty(), "у незнакомого пользователя чатов нет");
}

#[tokio::test]
async fn author_outside_members_still_sees_the_chat() {
    let ks = TestKeyspace::new().await;
    let store = ks.store().await;
    store.send(5, 42, "я тут новенький", &[7], 100).await.expect("отправка");
    assert_eq!(store.chats(42).await.expect("чаты"), [ChatSummary { chat: 5, last_at: 100 }], "автор видит чат, куда написал");
    assert_eq!(store.chats(7).await.expect("чаты"), [ChatSummary { chat: 5, last_at: 100 }]);
}

#[tokio::test]
async fn one_store_is_shared_between_tasks() {
    let ks = TestKeyspace::new().await;
    let store = Arc::new(ks.store().await);
    let again = ks.store().await;
    drop(again);
    let mut tasks = Vec::new();
    for i in 0..20 {
        let store = Arc::clone(&store);
        tasks.push(tokio::spawn(async move { store.send(8, i, "параллельно", &[], 1000 + i64::from(i)).await }));
    }
    for t in tasks {
        t.await.expect("задача").expect("отправка");
    }
    let page = store.history(8, None, 100).await.expect("история");
    assert_eq!(page.items.len(), 20, "все двадцать параллельных отправок на месте");
}
