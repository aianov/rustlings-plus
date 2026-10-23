use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use worker_pool::{JobError, JobHandle, PoolClosed, ThreadPool};

fn within<T: Send + 'static>(
    secs: u64,
    what: &str,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (done, result) = mpsc::channel();
    thread::Builder::new()
        .name(String::from("test-watch"))
        .spawn(move || {
            let _ = done.send(work());
        })
        .expect("ОС должна дать тесту поток");
    match result.recv_timeout(Duration::from_secs(secs)) {
        Ok(value) => value,
        Err(RecvTimeoutError::Timeout) => {
            panic!("{what}: нет ответа за {secs} с, похоже на зависание")
        }
        Err(RecvTimeoutError::Disconnected) => {
            panic!("{what}: поток теста упал, причина в сообщении выше")
        }
    }
}

fn pool(workers: usize) -> ThreadPool {
    ThreadPool::new(workers).expect("пул должен запускать рабочие потоки")
}

fn meet(arrived: &AtomicUsize, expected: usize) -> bool {
    arrived.fetch_add(1, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(3);
    while arrived.load(Ordering::SeqCst) < expected {
        if Instant::now() > deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(1));
    }
    true
}

fn counting(pool: &ThreadPool, done: &Arc<AtomicUsize>, jobs: usize, pause: u64) {
    for _ in 0..jobs {
        let done = Arc::clone(done);
        pool.execute(move || {
            thread::sleep(Duration::from_millis(pause));
            done.fetch_add(1, Ordering::SeqCst);
        })
        .expect("открытый пул принимает задания");
    }
}

#[test]
fn pool_lives_in_shared_state_and_errors_are_errors() {
    fn shared<T: Send + Sync>() {}
    fn error<E: Error + Send + Sync + 'static>() {}
    shared::<ThreadPool>();
    error::<PoolClosed>();
    error::<JobError>();
    assert!(
        !PoolClosed.to_string().is_empty() && !JobError::Panicked.to_string().is_empty(),
        "у ошибок пула есть текст для человека"
    );
}

#[test]
fn new_starts_the_requested_workers() {
    let (three, zero) = within(5, "запуск и остановка пула", || {
        (pool(3).workers(), pool(0).workers())
    });
    assert_eq!(three, 3, "ThreadPool::new(3) запускает три воркера");
    assert_eq!(
        zero, 1,
        "пул без воркеров никогда ничего не выполнит, поэтому new(0) запускает один"
    );
}

#[test]
fn submit_brings_results_back() {
    let results = within(5, "20 заданий через submit и join", || {
        let pool = pool(4);
        let handles: Vec<JobHandle<u64>> = (0..20u64)
            .map(|i| {
                pool.submit(move || i * i)
                    .expect("открытый пул принимает задания")
            })
            .collect();
        handles.into_iter().map(JobHandle::join).collect::<Vec<_>>()
    });
    let expected: Vec<Result<u64, JobError>> = (0..20u64).map(|i| Ok(i * i)).collect();
    assert_eq!(
        results, expected,
        "каждый JobHandle::join отдаёт результат своего задания"
    );
}

#[test]
fn workers_run_jobs_at_the_same_time() {
    let met = within(
        10,
        "четыре задания на четырёх воркерах",
        || {
            let pool = pool(4);
            let arrived = Arc::new(AtomicUsize::new(0));
            let handles: Vec<JobHandle<bool>> = (0..4)
                .map(|_| {
                    let arrived = Arc::clone(&arrived);
                    pool.submit(move || meet(&arrived, 4))
                        .expect("открытый пул принимает задания")
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join() == Ok(true))
                .collect::<Vec<bool>>()
        },
    );
    assert!(
        met.iter().all(|&met| met),
        "четыре задания на пуле из четырёх воркеров должны идти одновременно, а встретились не все: \
         {met:?}. Похоже, задание выполняется, пока воркер держит замок очереди"
    );
}

#[test]
fn panic_in_a_job_keeps_the_worker() {
    let (failed, met, workers) = within(10, "задания с паникой", || {
        let pool = pool(2);
        let handles: Vec<JobHandle<u32>> = (0..2)
            .map(|_| {
                pool.submit(|| -> u32 { panic!("задание падает нарочно") })
                    .expect("открытый пул принимает задания")
            })
            .collect();
        let failed: Vec<Result<u32, JobError>> = handles.into_iter().map(JobHandle::join).collect();
        let arrived = Arc::new(AtomicUsize::new(0));
        let handles: Vec<JobHandle<bool>> = (0..2)
            .map(|_| {
                let arrived = Arc::clone(&arrived);
                pool.submit(move || meet(&arrived, 2))
                    .expect("пул после паники принимает задания")
            })
            .collect();
        let met = handles.into_iter().all(|handle| handle.join() == Ok(true));
        (failed, met, pool.workers())
    });
    assert_eq!(
        failed,
        [Err(JobError::Panicked), Err(JobError::Panicked)],
        "паника внутри задания приходит в join как JobError::Panicked"
    );
    assert!(
        met,
        "после двух паник оба воркера живы и снова берут задания одновременно: лови панику \
         задания через catch_unwind, а не роняй поток"
    );
    assert_eq!(workers, 2, "паника не меняет число воркеров");
}

#[test]
fn wait_idle_waits_for_running_jobs_too() {
    within(2, "wait_idle на пустом пуле", || {
        pool(2).wait_idle()
    });
    let (done, pending) = within(5, "wait_idle после 30 заданий", || {
        let pool = pool(3);
        let done = Arc::new(AtomicUsize::new(0));
        counting(&pool, &done, 30, 10);
        pool.wait_idle();
        (done.load(Ordering::SeqCst), pool.pending())
    });
    assert_eq!(
        done, 30,
        "wait_idle возвращается, когда все задания доделаны, а не когда опустела очередь: \
         последние ещё работали"
    );
    assert_eq!(pending, 0, "после wait_idle очередь пуста");
}

#[test]
fn pending_counts_jobs_waiting_in_the_queue() {
    let (queued, after) = within(
        10,
        "очередь за занятым воркером",
        || {
            let pool = pool(1);
            let (started_tx, started) = mpsc::channel();
            let (open, gate) = mpsc::channel::<()>();
            pool.execute(move || {
                let _ = started_tx.send(());
                let _ = gate.recv_timeout(Duration::from_secs(5));
            })
            .expect("открытый пул принимает задания");
            started
                .recv_timeout(Duration::from_secs(5))
                .expect("первое задание должно начаться");
            for _ in 0..3 {
                pool.execute(|| {}).expect("открытый пул принимает задания");
            }
            let queued = pool.pending();
            let _ = open.send(());
            pool.wait_idle();
            (queued, pool.pending())
        },
    );
    assert_eq!(
        queued, 3,
        "pending считает задания, которые ждут в очереди: воркер занят первым, ещё три ждут"
    );
    assert_eq!(after, 0, "когда пул отработал, pending снова 0");
}

#[test]
fn close_rejects_new_jobs_and_finishes_the_queue() {
    let (execute, submit, done) = within(10, "close с полной очередью", || {
        let pool = pool(1);
        let (open, gate) = mpsc::channel::<()>();
        pool.execute(move || {
            let _ = gate.recv_timeout(Duration::from_secs(5));
        })
        .expect("открытый пул принимает задания");
        let done = Arc::new(AtomicUsize::new(0));
        counting(&pool, &done, 5, 0);
        pool.close();
        let execute = pool.execute(|| {});
        let submit = pool.submit(|| 1).err();
        let _ = open.send(());
        drop(pool);
        (execute, submit, done.load(Ordering::SeqCst))
    });
    assert_eq!(
        execute,
        Err(PoolClosed),
        "после close execute отвечает PoolClosed"
    );
    assert_eq!(
        submit,
        Some(PoolClosed),
        "после close submit тоже отвечает PoolClosed"
    );
    assert_eq!(
        done, 5,
        "close не выбрасывает очередь: задания, поставленные до close, доделываются"
    );
}

#[test]
fn drop_finishes_the_queue_and_stops_workers() {
    let done = within(5, "drop пула с очередью", || {
        let pool = pool(2);
        let done = Arc::new(AtomicUsize::new(0));
        counting(&pool, &done, 30, 3);
        drop(pool);
        done.load(Ordering::SeqCst)
    });
    assert_eq!(
        done, 30,
        "Drop пула закрывает очередь и ждёт воркеров (join): всё, что поставили, доделано"
    );
}
