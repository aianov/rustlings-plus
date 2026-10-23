use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use worker_pool::{JobError, PoolClosed, ThreadPool};

const SRC: &str = include_str!("../src/lib.rs");

fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(value) => value,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: не уложилось в {secs} с, похоже на зависание"),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: поток упал с паникой"),
    }
}

fn rendezvous(started: &AtomicUsize, parties: usize) -> bool {
    started.fetch_add(1, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(3);
    while started.load(Ordering::SeqCst) < parties {
        if Instant::now() > deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(1));
    }
    true
}

fn all_workers_alive(pool: &ThreadPool, parties: usize) -> bool {
    let started = Arc::new(AtomicUsize::new(0));
    let met = Arc::new(AtomicUsize::new(0));
    for _ in 0..parties {
        let started = Arc::clone(&started);
        let met = Arc::clone(&met);
        pool.execute(move || {
            if rendezvous(&started, parties) {
                met.fetch_add(1, Ordering::SeqCst);
            }
        })
        .unwrap();
    }
    pool.wait_idle();
    met.load(Ordering::SeqCst) == parties
}

#[test]
fn every_job_runs() {
    let count = within(10, "200 заданий", || {
        let pool = ThreadPool::new(4);
        let count = Arc::new(AtomicUsize::new(0));
        for _ in 0..200 {
            let count = Arc::clone(&count);
            pool.execute(move || {
                count.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        }
        pool.wait_idle();
        count.load(Ordering::SeqCst)
    });
    assert_eq!(count, 200, "после wait_idle все задания должны быть выполнены");
}

#[test]
fn workers_really_run_in_parallel() {
    let ok = within(10, "параллельность", || {
        let pool = ThreadPool::new(4);
        assert_eq!(pool.workers(), 4);
        all_workers_alive(&pool, 4)
    });
    assert!(ok, "четыре задания должны идти одновременно на четырёх рабочих");
}

#[test]
fn zero_workers_means_one() {
    let workers = within(5, "ноль рабочих", || {
        let pool = ThreadPool::new(0);
        let handle = pool.submit(|| 5).unwrap();
        assert_eq!(handle.join(), Ok(5));
        pool.workers()
    });
    assert_eq!(workers, 1);
}

#[test]
fn submit_returns_results_through_the_handle() {
    let results = within(10, "submit", || {
        let pool = ThreadPool::new(3);
        let handles: Vec<_> = (0..10_u64).map(|n| pool.submit(move || n * n).unwrap()).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>()
    });
    assert_eq!(results, vec![0, 1, 4, 9, 16, 25, 36, 49, 64, 81]);
}

#[test]
fn jobs_do_not_run_on_the_callers_thread() {
    let same = within(5, "поток задания", || {
        let pool = ThreadPool::new(2);
        let here = thread::current().id();
        pool.submit(move || thread::current().id() == here).unwrap().join().unwrap()
    });
    assert!(!same, "задание должно идти на рабочем потоке");
}

#[test]
fn a_panicking_job_is_reported_and_workers_survive() {
    let (err, ok) = within(15, "паника в задании", || {
        let pool = ThreadPool::new(3);
        let err = pool.submit(|| -> u8 { panic!("задание упало") }).unwrap().join();
        for _ in 0..3 {
            pool.execute(|| panic!("ещё одно падение")).unwrap();
        }
        pool.wait_idle();
        (err, all_workers_alive(&pool, 3))
    });
    assert_eq!(err, Err(JobError::Panicked), "паника задания должна стать JobError::Panicked");
    assert!(ok, "после паник должны остаться все три рабочих");
}

#[test]
fn wait_idle_waits_for_running_jobs() {
    let done = within(10, "wait_idle", || {
        let pool = ThreadPool::new(2);
        let done = Arc::new(AtomicUsize::new(0));
        for _ in 0..4 {
            let done = Arc::clone(&done);
            pool.execute(move || {
                thread::sleep(Duration::from_millis(60));
                done.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        }
        pool.wait_idle();
        done.load(Ordering::SeqCst)
    });
    assert_eq!(done, 4, "wait_idle вернулся, пока задания ещё шли");
}

#[test]
fn wait_idle_on_an_idle_pool_returns_at_once() {
    within(2, "wait_idle без заданий", || {
        let pool = ThreadPool::new(2);
        pool.wait_idle();
        pool.wait_idle();
    });
}

#[test]
fn pending_counts_only_queued_jobs() {
    let (queued, after) = within(10, "pending", || {
        let pool = ThreadPool::new(1);
        let gate = Arc::new(AtomicBool::new(false));
        let started = Arc::new(AtomicBool::new(false));
        {
            let gate = Arc::clone(&gate);
            let started = Arc::clone(&started);
            pool.execute(move || {
                started.store(true, Ordering::SeqCst);
                let deadline = Instant::now() + Duration::from_secs(3);
                while !gate.load(Ordering::SeqCst) && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(1));
                }
            })
            .unwrap();
        }
        while !started.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(1));
        }
        for _ in 0..5 {
            pool.execute(|| {}).unwrap();
        }
        let queued = pool.pending();
        gate.store(true, Ordering::SeqCst);
        pool.wait_idle();
        (queued, pool.pending())
    });
    assert_eq!(queued, 5, "пять заданий ждут, одно уже идёт");
    assert_eq!(after, 0);
}

#[test]
fn dropping_the_pool_finishes_everything_queued() {
    let count = within(10, "drop", || {
        let count = Arc::new(AtomicUsize::new(0));
        {
            let pool = ThreadPool::new(1);
            for _ in 0..20 {
                let count = Arc::clone(&count);
                pool.execute(move || {
                    thread::sleep(Duration::from_millis(5));
                    count.fetch_add(1, Ordering::SeqCst);
                })
                .unwrap();
            }
        }
        count.load(Ordering::SeqCst)
    });
    assert_eq!(count, 20, "при остановке принятые задания не должны теряться");
}

#[test]
fn dropping_an_unused_pool_does_not_hang() {
    within(3, "drop пустого пула", || {
        drop(ThreadPool::new(4));
    });
}

#[test]
fn a_closed_pool_rejects_new_jobs_and_finishes_old_ones() {
    let (rejected, submit_rejected, done) = within(10, "close", || {
        let pool = ThreadPool::new(2);
        let done = Arc::new(AtomicUsize::new(0));
        for _ in 0..5 {
            let done = Arc::clone(&done);
            pool.execute(move || {
                thread::sleep(Duration::from_millis(20));
                done.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        }
        pool.close();
        let rejected = pool.execute(|| {});
        let submit_rejected = pool.submit(|| 1).is_err();
        pool.wait_idle();
        (rejected, submit_rejected, done.load(Ordering::SeqCst))
    });
    assert_eq!(rejected, Err(PoolClosed));
    assert!(submit_rejected, "submit в закрытый пул должен вернуть PoolClosed");
    assert_eq!(done, 5, "принятые до закрытия задания выполняются");
}

#[test]
fn many_callers_can_share_one_pool() {
    let count = within(15, "много вызывающих", || {
        let pool = Arc::new(ThreadPool::new(4));
        let count = Arc::new(AtomicUsize::new(0));
        let callers: Vec<_> = (0..4)
            .map(|_| {
                let pool = Arc::clone(&pool);
                let count = Arc::clone(&count);
                thread::spawn(move || {
                    for _ in 0..50 {
                        let count = Arc::clone(&count);
                        pool.execute(move || {
                            count.fetch_add(1, Ordering::SeqCst);
                        })
                        .unwrap();
                    }
                })
            })
            .collect();
        for caller in callers {
            caller.join().unwrap();
        }
        pool.wait_idle();
        count.load(Ordering::SeqCst)
    });
    assert_eq!(count, 200);
}

#[test]
fn results_keep_working_when_many_handles_are_joined_late() {
    let sum = within(10, "поздний join", || {
        let pool = ThreadPool::new(2);
        let shared = Arc::new(Mutex::new(0_u64));
        let handles: Vec<_> = (1..=20_u64)
            .map(|n| {
                let shared = Arc::clone(&shared);
                pool.submit(move || {
                    *shared.lock().unwrap() += n;
                    n
                })
                .unwrap()
            })
            .collect();
        thread::sleep(Duration::from_millis(50));
        let total: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
        assert_eq!(*shared.lock().unwrap(), total);
        total
    });
    assert_eq!(sum, 210);
}

#[test]
fn waiting_is_built_on_condvar_not_polling() {
    let code: String = SRC
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(code.contains("Condvar"), "ожидание заданий и wait_idle должны стоять на Condvar");
    assert!(!code.contains("sleep"), "sleep в пуле не нужен");
    assert!(!code.contains("yield_now"), "yield_now в пуле не нужен");
    assert!(!code.contains("spin_loop"), "активное ожидание не нужно");
}
