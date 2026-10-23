use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use journal::{Journal, JournalError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Entry {
    Added { id: u64, name: String },
    Removed { id: u64 },
}

fn path(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m33-journal")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    dir.join("entries.ndjson")
}

#[tokio::test]
async fn entries_come_back_in_order() {
    let file = path("order");
    let (journal, old) = Journal::<Entry>::open(Some(&file))
        .await
        .expect("журнал открывается");
    assert!(
        old.is_empty() && journal.is_durable(),
        "новый журнал пуст и пишет на диск"
    );
    let entries = [
        Entry::Added {
            id: 1,
            name: "раз".to_string(),
        },
        Entry::Added {
            id: 2,
            name: "два".to_string(),
        },
        Entry::Removed { id: 1 },
    ];
    for entry in &entries {
        journal.append(entry).await.expect("запись");
    }
    drop(journal);
    let (_, back) = Journal::<Entry>::open(Some(&file))
        .await
        .expect("журнал открывается");
    assert_eq!(
        back, entries,
        "открытие отдаёт записи в том порядке, в каком их писали"
    );
    let text = fs::read_to_string(&file).expect("файл");
    assert_eq!(
        text.lines().next(),
        Some(r#"{"event":"added","id":1,"name":"раз"}"#),
        "одна запись это одна строка JSON"
    );
}

#[tokio::test]
async fn torn_tail_is_cut_and_broken_middle_is_an_error() {
    let file = path("torn");
    let (journal, _) = Journal::<Entry>::open(Some(&file)).await.expect("журнал");
    journal
        .append(&Entry::Removed { id: 7 })
        .await
        .expect("запись");
    drop(journal);
    let mut text = fs::read_to_string(&file).expect("файл");
    text.push_str(r#"{"event":"rem"#);
    fs::write(&file, &text).expect("обрыв дописан");
    let (journal, back) = Journal::<Entry>::open(Some(&file))
        .await
        .expect("обрыв не мешает");
    assert_eq!(back, [Entry::Removed { id: 7 }]);
    journal
        .append(&Entry::Removed { id: 8 })
        .await
        .expect("запись после обрыва");
    drop(journal);
    let (_, back) = Journal::<Entry>::open(Some(&file)).await.expect("журнал");
    assert_eq!(
        back,
        [Entry::Removed { id: 7 }, Entry::Removed { id: 8 }],
        "оборванный хвост отрезан при открытии, новая запись легла целой строкой"
    );
    fs::write(
        &file,
        "{\"event\":\"removed\",\"id\":1}\nчепуха\n{\"event\":\"removed\",\"id\":2}\n",
    )
    .expect("порча посередине");
    match Journal::<Entry>::open(Some(&file)).await {
        Err(JournalError::Broken { line, .. }) => {
            assert_eq!(line, 2, "ошибка называет испорченную строку")
        }
        Err(other) => panic!("ждали Broken, а пришло {other}"),
        Ok(..) => panic!("испорченная середина это ошибка, а не тихий пропуск"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_appends_stay_whole_lines() {
    let file = path("parallel");
    let (journal, _) = Journal::<Entry>::open(Some(&file)).await.expect("журнал");
    let journal = Arc::new(journal);
    let mut tasks = Vec::new();
    for id in 0..64 {
        let journal = Arc::clone(&journal);
        tasks.push(tokio::spawn(async move {
            let name = "x".repeat(200);
            journal.append(&Entry::Added { id, name }).await
        }));
    }
    for task in tasks {
        task.await.expect("задача").expect("запись");
    }
    drop(journal);
    let (_, back) = Journal::<Entry>::open(Some(&file)).await.expect("журнал");
    assert_eq!(back.len(), 64, "ни одна строка не перемешалась с соседней");
}

#[tokio::test]
async fn memory_journal_keeps_nothing() {
    let (journal, old) = Journal::<Entry>::open(None).await.expect("журнал в памяти");
    assert!(old.is_empty() && !journal.is_durable());
    journal
        .append(&Entry::Removed { id: 1 })
        .await
        .expect("запись в никуда");
}
