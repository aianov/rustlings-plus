use std::cell::RefCell;
use std::future::pending;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use futures::future::{Either, join_all, select};
use minirt::{Runtime, now, sleep, spawn, yield_now};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn within<F: FnOnce() + Send + 'static>(secs: u64, body: F) {
    let (done, finished) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .spawn(move || {
            body();
            let _ = done.send(());
        })
        .unwrap();
    match finished.recv_timeout(Duration::from_secs(secs)) {
        Ok(()) => worker.join().unwrap(),
        Err(mpsc::RecvTimeoutError::Disconnected) => match worker.join() {
            Err(panic) => std::panic::resume_unwind(panic),
            Ok(()) => unreachable!(),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("рантайм завис: сценарий не закончился за {secs} с"),
    }
}

type Log = Rc<RefCell<Vec<String>>>;

fn log() -> Log {
    Rc::new(RefCell::new(Vec::new()))
}

fn push(log: &Log, text: &str) {
    log.borrow_mut().push(text.to_string());
}

#[test]
fn block_on_returns_the_value_of_the_main_future() {
    within(10, || {
        let rt = Runtime::paused();
        assert_eq!(rt.block_on(async { 6 * 7 }), 42, "block_on обязан вернуть результат главного future");
        assert_eq!(rt.block_on(async { "ещё раз" }), "ещё раз", "рантайм пригоден для повторных block_on");
    });
}

#[test]
fn spawned_tasks_start_in_spawn_order() {
    within(10, || {
        let rt = Runtime::paused();
        let log = log();
        rt.block_on({
            let log = log.clone();
            async move {
                for name in ["a", "b", "c"] {
                    let log = log.clone();
                    spawn(async move { push(&log, name) });
                }
                yield_now().await;
            }
        });
        assert_eq!(*log.borrow(), vec!["a", "b", "c"], "задачи идут по очереди создания");
    });
}

#[test]
fn yield_now_makes_tasks_take_turns() {
    within(10, || {
        let rt = Runtime::paused();
        let log = log();
        rt.block_on({
            let log = log.clone();
            async move {
                let mut handles = Vec::new();
                for name in ["a", "b"] {
                    let log = log.clone();
                    handles.push(spawn(async move {
                        for i in 0..3 {
                            push(&log, &format!("{name}{i}"));
                            yield_now().await;
                        }
                    }));
                }
                for handle in handles {
                    handle.await;
                }
            }
        });
        assert_eq!(*log.borrow(), vec!["a0", "b0", "a1", "b1", "a2", "b2"], "после yield_now задача встаёт в конец очереди, и соседи ходят по кругу");
    });
}

#[test]
fn sleepers_wake_in_deadline_order_and_the_clock_jumps() {
    within(10, || {
        let rt = Runtime::paused();
        let log = log();
        rt.block_on({
            let log = log.clone();
            async move {
                let mut handles = Vec::new();
                for (name, pause) in [("a", 30), ("b", 10), ("c", 20)] {
                    let log = log.clone();
                    handles.push(spawn(async move {
                        sleep(ms(pause)).await;
                        push(&log, name);
                    }));
                }
                join_all(handles).await;
            }
        });
        assert_eq!(*log.borrow(), vec!["b", "c", "a"], "просыпаются по порядку сроков, а не создания");
        assert_eq!(rt.elapsed(), ms(30), "часы с паузой прыгают к ближайшему сроку: всего 30 мс виртуального времени");
    });
}

#[test]
fn sequential_sleeps_add_up_and_parallel_ones_overlap() {
    within(10, || {
        let rt = Runtime::paused();
        rt.block_on(async {
            sleep(ms(100)).await;
            sleep(ms(200)).await;
        });
        assert_eq!(rt.elapsed(), ms(300), "два сна подряд складываются");
        let rt = Runtime::paused();
        rt.block_on(async {
            let handles: Vec<_> = (0..100).map(|_| spawn(sleep(ms(50)))).collect();
            join_all(handles).await;
        });
        assert_eq!(rt.elapsed(), ms(50), "сто одновременных снов на 50 мс занимают 50 мс, а не 5 секунд");
    });
}

#[test]
fn equal_deadlines_wake_in_creation_order() {
    within(10, || {
        let rt = Runtime::paused();
        let log = log();
        rt.block_on({
            let log = log.clone();
            async move {
                let handles: Vec<_> = (0..5)
                    .map(|i| {
                        let log = log.clone();
                        spawn(async move {
                            sleep(ms(10)).await;
                            push(&log, &i.to_string());
                        })
                    })
                    .collect();
                join_all(handles).await;
            }
        });
        assert_eq!(*log.borrow(), vec!["0", "1", "2", "3", "4"], "одинаковые сроки: кто раньше лёг спать, тот раньше проснулся");
    });
}

#[test]
fn a_join_handle_gives_the_result_and_a_dropped_one_cancels_nothing() {
    within(10, || {
        let rt = Runtime::paused();
        let flag = Rc::new(RefCell::new(false));
        let value = rt.block_on({
            let flag = flag.clone();
            async move {
                let quick = spawn(async { 5 });
                drop(spawn(async move {
                    sleep(ms(10)).await;
                    *flag.borrow_mut() = true;
                }));
                sleep(ms(50)).await;
                quick.await
            }
        });
        assert_eq!(value, 5, "результат завершившейся раньше задачи ждёт в handle");
        assert!(*flag.borrow(), "выброшенный handle не отменяет задачу");
    });
}

#[test]
fn block_on_does_not_wait_for_forgotten_tasks() {
    within(10, || {
        let rt = Runtime::paused();
        let out = rt.block_on(async {
            spawn(pending::<()>());
            spawn(async {
                sleep(Duration::from_secs(3600)).await;
            });
            yield_now().await;
            "готово"
        });
        assert_eq!(out, "готово", "главное future кончилось: block_on возвращается, задачи-хвосты его не держат");
        assert_eq!(rt.elapsed(), Duration::ZERO, "до часового сна дело не дошло, часы не должны были прыгать");
    });
}

#[test]
fn tasks_spawn_tasks_and_wait_for_them() {
    within(10, || {
        let rt = Runtime::paused();
        let total = rt.block_on(async {
            let parent = spawn(async {
                let a = spawn(async {
                    sleep(ms(10)).await;
                    1
                });
                let b = spawn(async {
                    sleep(ms(20)).await;
                    2
                });
                a.await + b.await
            });
            parent.await * 100
        });
        assert_eq!(total, 300, "задача порождает задачи и ждёт их");
        assert_eq!(rt.elapsed(), ms(20), "дети спали параллельно");
    });
}

#[test]
fn a_dropped_sleep_leaves_no_timer_behind() {
    within(10, || {
        let rt = Runtime::paused();
        rt.block_on(async {
            let long = Box::pin(sleep(ms(1000)));
            let short = Box::pin(sleep(ms(10)));
            match select(long, short).await {
                Either::Right(_) => {}
                Either::Left(_) => panic!("короткий сон должен был выиграть"),
            }
        });
        assert_eq!(rt.elapsed(), ms(10), "победил короткий сон");
        assert_eq!(rt.live_timers(), 0, "проигравший Sleep выброшен: его запись в драйвере времени должна исчезнуть");
        rt.block_on(async {
            let mut sleeps = Vec::new();
            for _ in 0..3 {
                let mut s = Box::pin(sleep(ms(500)));
                let _ = futures::poll!(&mut s);
                sleeps.push(s);
            }
            assert_eq!(minirt::now(), ms(10));
        });
        assert_eq!(rt.live_timers(), 0, "все Sleep выброшены вместе с вектором: таймеров быть не должно");
    });
}

#[test]
fn a_repolled_sleep_with_another_waker_still_wakes() {
    within(10, || {
        let rt = Runtime::paused();
        rt.block_on(async {
            let mut nap = Box::pin(sleep(ms(40)));
            let task = spawn(async move {
                nap.as_mut().await;
                now()
            });
            assert_eq!(task.await, ms(40), "Sleep, опрошенный сначала в одной задаче, потом в другой, просыпается в срок");
        });
    });
}

#[test]
fn ten_thousand_tasks_finish_quickly() {
    within(20, || {
        let rt = Runtime::paused();
        let counter = Rc::new(RefCell::new(0u32));
        rt.block_on({
            let counter = counter.clone();
            async move {
                let handles: Vec<_> = (0..10_000)
                    .map(|i| {
                        let counter = counter.clone();
                        spawn(async move {
                            yield_now().await;
                            sleep(ms(i % 7)).await;
                            yield_now().await;
                            *counter.borrow_mut() += 1;
                        })
                    })
                    .collect();
                join_all(handles).await;
            }
        });
        assert_eq!(*counter.borrow(), 10_000, "все десять тысяч задач дошли до конца");
    });
}

struct Oneshot<T> {
    state: Arc<Mutex<(Option<T>, Option<Waker>)>>,
}

impl<T> std::future::Future for Oneshot<T> {
    type Output = T;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut state = self.state.lock().unwrap();
        match state.0.take() {
            Some(value) => Poll::Ready(value),
            None => {
                state.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

#[test]
fn a_waker_called_from_another_thread_wakes_an_idle_runtime() {
    within(10, || {
        let state = Arc::new(Mutex::new((None, None::<Waker>)));
        let remote = state.clone();
        let sender = std::thread::Builder::new()
            .spawn(move || {
                std::thread::sleep(ms(50));
                let mut state = remote.lock().unwrap();
                state.0 = Some("из другого потока");
                if let Some(waker) = state.1.take() {
                    waker.wake();
                }
            })
            .unwrap();
        let rt = Runtime::new();
        let got = rt.block_on(Oneshot { state });
        assert_eq!(got, "из другого потока", "будильник из чужого потока должен разбудить спящий рантайм");
        sender.join().unwrap();
    });
}

#[test]
fn the_real_clock_really_waits() {
    within(10, || {
        let rt = Runtime::new();
        let started = std::time::Instant::now();
        rt.block_on(async {
            let a = spawn(sleep(ms(60)));
            let b = spawn(sleep(ms(60)));
            a.await;
            b.await;
        });
        let took = started.elapsed();
        assert!(took >= ms(60), "рантайм с настоящими часами проснулся раньше срока: {took:?}");
        assert!(took < ms(400), "два параллельных сна по 60 мс заняли {took:?}: похоже, они шли по очереди или рантайм крутился вхолостую");
        assert!(rt.elapsed() >= ms(60), "elapsed настоящих часов должен идти");
    });
}

#[test]
fn spawning_outside_the_runtime_panics_clearly() {
    within(10, || {
        let result = std::panic::catch_unwind(|| {
            drop(sleep(ms(1)));
        });
        assert!(result.is_err(), "sleep вне block_on должен паниковать, а не молча работать");
    });
}
