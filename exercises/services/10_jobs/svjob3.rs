// После публикации поста нужно разослать уведомление подписчикам, и это отдельная фоновая задача. Воркер быстрый:
// он берёт задачу сразу и идёт в базу за постом. Но иногда он не находит пост, а иногда после падения сервиса пост
// есть, а задачи нет, и подписчики не получают ничего. Цель: задача не обгоняет сохранение поста, не появляется
// при неудачном коммите и не теряется, если сервис упал или очередь не ответила между коммитом и постановкой.
// Заготовки для надёжной постановки в файле уже есть.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

fn main() {}

#[derive(Debug, PartialEq)]
pub enum DbError {
    CommitFailed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: String,
    pub post_id: u64,
}

#[derive(Default)]
struct Tables {
    posts: HashMap<u64, String>,
    outbox: Vec<Job>,
}

#[derive(Default)]
pub struct Db {
    tables: Mutex<Tables>,
    fail_commit: AtomicBool,
}

pub struct Tx<'a> {
    db: &'a Db,
    posts: Vec<(u64, String)>,
    jobs: Vec<Job>,
}

impl Db {
    pub fn begin(&self) -> Tx<'_> {
        Tx { db: self, posts: Vec::new(), jobs: Vec::new() }
    }

    pub fn post_exists(&self, id: u64) -> bool {
        self.tables.lock().unwrap().posts.contains_key(&id)
    }

    pub fn fail_next_commit(&self) {
        self.fail_commit.store(true, Ordering::SeqCst);
    }

    pub fn pending_jobs(&self) -> Vec<Job> {
        self.tables.lock().unwrap().outbox.clone()
    }

    pub fn mark_sent(&self, job_id: &str) {
        self.tables.lock().unwrap().outbox.retain(|j| j.id != job_id);
    }
}

impl Tx<'_> {
    pub fn insert_post(&mut self, id: u64, text: &str) {
        self.posts.push((id, text.to_string()));
    }

    pub fn add_job(&mut self, job: Job) {
        self.jobs.push(job);
    }

    pub fn commit(self) -> Result<(), DbError> {
        if self.db.fail_commit.swap(false, Ordering::SeqCst) {
            return Err(DbError::CommitFailed);
        }
        let mut tables = self.db.tables.lock().unwrap();
        tables.posts.extend(self.posts);
        tables.outbox.extend(self.jobs);
        Ok(())
    }
}

pub struct Queue {
    db: Arc<Db>,
    jobs: Mutex<Vec<Job>>,
    misses: Mutex<Vec<u64>>,
    fail_next: AtomicBool,
}

impl Queue {
    pub fn new(db: Arc<Db>) -> Queue {
        Queue { db, jobs: Mutex::new(Vec::new()), misses: Mutex::new(Vec::new()), fail_next: AtomicBool::new(false) }
    }

    pub fn fail_next_enqueue(&self) {
        self.fail_next.store(true, Ordering::SeqCst);
    }

    pub fn enqueue(&self, job: &Job) -> Result<(), String> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err("очередь не ответила".to_string());
        }
        let mut jobs = self.jobs.lock().unwrap();
        if jobs.iter().any(|j| j.id == job.id) {
            return Ok(());
        }
        if !self.db.post_exists(job.post_id) {
            self.misses.lock().unwrap().push(job.post_id);
        }
        jobs.push(job.clone());
        Ok(())
    }

    pub fn queued(&self) -> usize {
        self.jobs.lock().unwrap().len()
    }

    pub fn misses(&self) -> Vec<u64> {
        self.misses.lock().unwrap().clone()
    }
}

pub fn flush_outbox(db: &Db, queue: &Queue) -> usize {
    let mut sent = 0;
    for job in db.pending_jobs() {
        if queue.enqueue(&job).is_ok() {
            db.mark_sent(&job.id);
            sent += 1;
        }
    }
    sent
}

pub fn create_post(db: &Db, queue: &Queue, id: u64, text: &str) -> Result<(), DbError> {
    let mut tx = db.begin();
    tx.insert_post(id, text);
    let _ = queue.enqueue(&Job { id: format!("notify:{id}"), post_id: id });
    tx.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<Db>, Queue) {
        let db = Arc::new(Db::default());
        let queue = Queue::new(Arc::clone(&db));
        (db, queue)
    }

    #[test]
    fn the_worker_never_looks_for_a_post_that_is_not_saved_yet() {
        let (db, queue) = setup();
        for id in 1..=20 {
            create_post(&db, &queue, id, "текст").expect("пост создан");
        }
        assert_eq!(queue.queued(), 20, "по задаче на каждый пост");
        assert!(queue.misses().is_empty(), "быстрый воркер не нашёл посты {:?}: задача попала в очередь раньше коммита", queue.misses());
    }

    #[test]
    fn a_failed_commit_leaves_no_job_behind() {
        let (db, queue) = setup();
        db.fail_next_commit();
        assert_eq!(create_post(&db, &queue, 1, "текст"), Err(DbError::CommitFailed), "коммит упал, ошибка доходит до вызывающего");
        assert_eq!(queue.queued(), 0, "поста нет, значит, и уведомлять не о чем: задача не должна попасть в очередь");
        assert!(db.pending_jobs().is_empty(), "и в таблице задач строки быть не должно");
    }

    #[test]
    fn a_job_survives_when_the_queue_is_down_right_after_the_commit() {
        let (db, queue) = setup();
        queue.fail_next_enqueue();
        assert_eq!(create_post(&db, &queue, 7, "текст"), Ok(()), "пост сохранён, ответ успешный: об очереди клиент знать не должен");
        assert!(db.post_exists(7), "пост на месте");
        assert_eq!(queue.queued(), 0, "очередь не ответила, задачи в ней ещё нет");
        assert_eq!(db.pending_jobs().len(), 1, "но запись о задаче сохранена в той же транзакции, что и пост");
        assert_eq!(flush_outbox(&db, &queue), 1, "позже курьер доставляет её");
        assert_eq!(queue.queued(), 1, "задача в очереди");
        assert_eq!(flush_outbox(&db, &queue), 0, "а второй проход ничего не повторяет");
        assert_eq!(queue.queued(), 1, "дубля нет");
    }

    #[test]
    fn a_delivered_job_is_crossed_off() {
        let (db, queue) = setup();
        create_post(&db, &queue, 3, "текст").unwrap();
        assert!(db.pending_jobs().is_empty(), "доставленная задача вычеркнута из таблицы, иначе курьер будет возить её вечно");
    }
}
