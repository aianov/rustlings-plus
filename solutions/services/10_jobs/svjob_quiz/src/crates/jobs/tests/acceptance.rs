use jobs::batch::{Checkpoints, run_checkpointed};
use jobs::lease::LeaseStore;
use jobs::periodic::Periodic;
use jobs::policy::{Action, JobError, MAX_ATTEMPTS, backoff_ms, decide};
use jobs::queue::{Delivery, Job, MemQueue};
use jobs::scheduler::leader_tick;
use jobs::worker::Worker;
use std::collections::{BTreeMap, BTreeSet};

fn temporary() -> Result<(), JobError> {
    Err(JobError::Temporary("таймаут".to_string()))
}

fn worker_with(jobs: Vec<Job>, prefetch: usize) -> Worker {
    let mut queue = MemQueue::new(30_000);
    for job in jobs {
        queue.enqueue(job);
    }
    Worker::new(queue, prefetch)
}

fn drive(worker: &mut Worker, handler: &mut impl FnMut(&Delivery) -> Result<(), JobError>, from: u64, until: u64) {
    let mut now = from;
    while now <= until {
        while worker.tick(now, handler) {}
        now += 100;
    }
}

#[test]
fn backoff_grows_from_a_second_to_a_minute_and_never_overflows() {
    let pauses: Vec<u64> = (1..=8).map(backoff_ms).collect();
    assert_eq!(pauses, vec![1000, 2000, 4000, 8000, 16_000, 32_000, 60_000, 60_000], "1 с, вдвое, потолок минута");
    for attempt in [0, 64, 65, 1000, u32::MAX] {
        let pause = backoff_ms(attempt);
        assert!((1000..=60_000).contains(&pause), "попытка {attempt}: пауза {pause} вышла за границы, возможно, переполнение");
    }
}

#[test]
fn decisions_follow_the_error_kind_and_the_attempt() {
    assert_eq!(decide(Ok(()), 3), Action::Done, "успех");
    assert_eq!(decide(temporary(), 2), Action::Retry { after_ms: backoff_ms(2) }, "временная ошибка: повтор с паузой по номеру");
    match decide(temporary(), MAX_ATTEMPTS) {
        Action::Dead { reason } => assert!(reason.contains("таймаут") && reason.contains('5'), "в причине исходная ошибка и число попыток: {reason}"),
        other => panic!("на последней попытке нужна мёртвая очередь, а не {other:?}"),
    }
    assert_eq!(
        decide(Err(JobError::Permanent("нет такого пользователя".to_string())), 1),
        Action::Dead { reason: "нет такого пользователя".to_string() },
        "постоянная ошибка не повторяется даже на первой попытке"
    );
}

#[test]
fn a_temporary_failure_is_retried_at_growing_intervals_and_then_succeeds() {
    let mut worker = worker_with(vec![Job::new("mail:1", "send", 0)], 1);
    let mut at = Vec::new();
    let mut now = 0;
    while now <= 60_000 && !worker.queue().is_empty() {
        let mut calls = 0;
        let mut handler = |_: &Delivery| {
            calls += 1;
            if at.len() < 3 { temporary() } else { Ok(()) }
        };
        if worker.tick(now, &mut handler) {
            at.push(now);
        }
        now += 100;
    }
    assert_eq!(at, vec![0, 1000, 3000, 7000], "попытки в моменты 0, +1 с, +2 с, +4 с");
    assert!(worker.queue().is_empty() && worker.queue().dead().is_empty(), "после успеха очередь и мёртвая очередь пусты");
}

#[test]
fn poison_goes_to_the_dead_queue_at_once_and_exhausted_retries_are_reported_with_the_reason() {
    let mut worker = worker_with(vec![Job::new("mail:gone", "send", 0), Job::new("mail:flaky", "send", 0)], 2);
    let mut calls: BTreeMap<String, u32> = BTreeMap::new();
    drive(
        &mut worker,
        &mut |d| {
            *calls.entry(d.job.id.clone()).or_default() += 1;
            if d.job.id == "mail:gone" { Err(JobError::Permanent("пользователь удалён".to_string())) } else { temporary() }
        },
        0,
        600_000,
    );
    assert_eq!(calls["mail:gone"], 1, "безнадёжная задача выполнена один раз");
    assert_eq!(calls["mail:flaky"], MAX_ATTEMPTS, "временная получила все положенные попытки");
    let dead: BTreeMap<&str, &str> = worker.queue().dead().iter().map(|(j, r)| (j.id.as_str(), r.as_str())).collect();
    assert!(dead["mail:gone"].contains("пользователь удалён"), "причина названа: {dead:?}");
    assert!(dead["mail:flaky"].contains("таймаут"), "причина последней неудачи не потеряна: {dead:?}");
    assert!(worker.queue().is_empty(), "в рабочей очереди ничего не осталось");
}

#[test]
fn delayed_jobs_wait_for_their_time_without_blocking_or_polling() {
    let mut worker = worker_with(vec![Job::new("remind", "remind", 90_000), Job::new("now", "send", 0)], 2);
    let mut ran: Vec<(String, u64, u32)> = Vec::new();
    let mut now = 0;
    while now <= 95_000 {
        let mut handler = |d: &Delivery| {
            ran.push((d.job.id.clone(), now, d.delivered));
            Ok(())
        };
        while worker.tick(now, &mut handler) {}
        now += 100;
    }
    assert_eq!(ran.len(), 2, "обе задачи выполнены: {ran:?}");
    assert_eq!((ran[0].0.as_str(), ran[0].1), ("now", 0), "готовая задача пошла сразу, отложенная ей не мешала");
    assert_eq!((ran[1].0.as_str(), ran[1].1), ("remind", 90_000), "отложенная выполнена ровно в срок, не раньше");
    assert!(ran[1].2 <= 2, "отложенную выдавали {} раз: ожидание должно быть одним возвратом, а не опросом", ran[1].2);
}

#[test]
fn shutdown_returns_everything_that_was_taken_but_not_started() {
    let jobs: Vec<Job> = (1..=5).map(|i| Job::new(&format!("j{i}"), "send", 0)).collect();
    let mut worker = worker_with(jobs, 5);
    let mut started = Vec::new();
    assert!(worker.tick(0, &mut |d| { started.push(d.job.id.clone()); Ok(()) }), "первая задача выполнена");
    worker.shutdown(0);
    assert!(!worker.tick(0, &mut |d| { started.push(d.job.id.clone()); Ok(()) }), "после остановки воркер новых задач не берёт");
    assert_eq!(started, vec!["j1"], "остальные даже не начаты");
    let mut other_instance = Vec::new();
    while let Some(d) = worker.queue_mut().take(0) {
        other_instance.push(d.job.id);
    }
    assert_eq!(other_instance, vec!["j2", "j3", "j4", "j5"], "невыполненные снова доступны другому экземпляру в тот же момент, а не через ack_wait");
}

#[test]
fn a_periodic_job_fires_once_after_a_pause_and_stays_on_its_grid() {
    let mut job = Periodic::new("auth-1", 60_000, 0);
    let first = job.next_at();
    assert!(job.due(first - 1).is_empty(), "до срока тишина");
    assert_eq!(job.due(first).len(), 1, "в срок одно срабатывание");
    let late = first + 100 * 60_000 + 5;
    assert_eq!(job.due(late).len(), 1, "после сна на сто периодов срабатывание всё равно одно");
    assert!(job.due(late).is_empty(), "и повторно в тот же момент нет");
    assert!(job.next_at() > late && job.next_at() <= late + 60_000, "следующий срок в пределах периода");
    assert_eq!((job.next_at() - first) % 60_000, 0, "и на прежней сетке");
}

#[test]
fn instances_spread_their_first_run_and_keep_the_offset_after_a_restart() {
    let starts: BTreeSet<u64> = (0..100).map(|i| Periodic::new(&format!("auth-{i}"), 60_000, 0).next_at()).collect();
    assert!(starts.len() >= 80, "сто экземпляров стартуют в {} разных моментах: разброса нет", starts.len());
    let a = Periodic::new("auth-7", 60_000, 1_000).next_at() - 1_000;
    let b = Periodic::new("auth-7", 60_000, 5_000_000).next_at() - 5_000_000;
    assert_eq!(a, b, "сдвиг зависит от имени и после перезапуска тот же");
}

#[test]
fn a_lease_has_one_owner_a_growing_term_and_an_expiry() {
    let mut store = LeaseStore::new(1000);
    let a = store.acquire("cleanup", "auth-1", 0).expect("свободная аренда");
    assert!(store.acquire("cleanup", "auth-2", 999).is_none(), "до истечения аренда занята");
    assert!(store.renew(&a, 900), "владелец продлевает вовремя");
    assert!(store.acquire("cleanup", "auth-2", 1899).is_none(), "продление сдвинуло срок: 900 + 1000");
    let b = store.acquire("cleanup", "auth-2", 1900).expect("на границе срока аренда уже свободна");
    assert!(b.term > a.term, "номер новой выдачи больше прежнего");
    assert!(!store.renew(&a, 1900), "старый владелец продлить чужую аренду не может");
    store.release(&a);
    assert!(store.acquire("cleanup", "auth-3", 1950).is_none(), "release старого владельца не освобождает аренду нового");
    store.release(&b);
    assert!(store.acquire("cleanup", "auth-3", 1951).is_some(), "release владельца освобождает аренду сразу");
    let c = store.acquire("other", "auth-1", 0).expect("другая работа");
    assert!(!store.renew(&c, 1000), "просроченную аренду продлить нельзя: она уже могла уйти другому");
}

fn cluster_runs(ticking: impl Fn(usize, u64) -> bool) -> Vec<u64> {
    let every = 600_000;
    let mut leases = LeaseStore::new(every);
    let mut instances: Vec<Periodic> = (1..=3).map(|i| Periodic::new(&format!("auth-{i}"), every, 0)).collect();
    let mut runs = Vec::new();
    let mut now = 0;
    while now < 3_600_000 {
        for (i, periodic) in instances.iter_mut().enumerate() {
            if ticking(i, now) {
                leader_tick(periodic, &mut leases, "cleanup", &format!("auth-{}", i + 1), now, &mut || runs.push(now / every));
            }
        }
        now += 1000;
    }
    runs
}

#[test]
fn three_instances_run_the_cleanup_once_per_period() {
    let runs = cluster_runs(|_, _| true);
    assert_eq!(runs, vec![0, 1, 2, 3, 4, 5], "за час при периоде в 10 минут ровно шесть чисток, по одной на период, а не по три: {runs:?}");
}

#[test]
fn a_dead_instance_does_not_stop_the_cleanup() {
    let runs = cluster_runs(|i, _| i != 0 && i != 1);
    assert_eq!(runs, vec![0, 1, 2, 3, 4, 5], "работает один экземпляр из трёх, чистка идёт по расписанию: {runs:?}");
}

#[test]
fn after_a_long_pause_of_the_whole_cluster_the_cleanup_runs_once() {
    let runs = cluster_runs(|_, now| !(600_000..3_000_000).contains(&now));
    assert_eq!(runs, vec![0, 5], "все спали сорок минут: пропущенные периоды не догоняются, после пробуждения одна чистка: {runs:?}");
}

#[derive(Default)]
struct Provider {
    calls: usize,
    fail_on_call: Option<usize>,
    fail_after_send: bool,
    seen_keys: BTreeSet<String>,
    received: BTreeMap<String, u32>,
}

impl Provider {
    fn send(&mut self, key: &str, to: &str) -> Result<(), String> {
        self.calls += 1;
        let failing = self.fail_on_call == Some(self.calls);
        if failing && !self.fail_after_send {
            return Err("почтовик моргнул".to_string());
        }
        if self.seen_keys.insert(key.to_string()) {
            *self.received.entry(to.to_string()).or_default() += 1;
        }
        if failing { Err("процесс упал сразу после отправки".to_string()) } else { Ok(()) }
    }
}

#[test]
fn a_crashed_mailing_continues_from_the_checkpoint_and_never_double_sends() {
    let users: Vec<String> = (1..=12).map(|i| format!("u{i:02}")).collect();
    let mut checkpoints = Checkpoints::default();
    let mut provider = Provider { fail_on_call: Some(5), ..Default::default() };
    let first = run_checkpointed("digest:d1", &users, &mut |k, to| provider.send(k, to), &mut checkpoints);
    assert!(first.is_err(), "пятое письмо не ушло: ошибка должна дойти до вызывающего");
    let calls_after_crash = provider.calls;
    let second = run_checkpointed("digest:d1", &users, &mut |k, to| provider.send(k, to), &mut checkpoints);
    assert_eq!(second, Ok(8), "доделан остаток, начиная с пятого пользователя");
    assert!(provider.calls - calls_after_crash <= 8, "повторный запуск сделал {} обращений: начинать с начала нельзя", provider.calls - calls_after_crash);
    assert!(users.iter().all(|u| provider.received.get(u) == Some(&1)), "каждый получил ровно одно письмо: {:?}", provider.received);
    let calls = provider.calls;
    assert_eq!(run_checkpointed("digest:d1", &users, &mut |k, to| provider.send(k, to), &mut checkpoints), Ok(0), "повторная выдача законченной задачи ничего не делает");
    assert_eq!(provider.calls, calls, "и к почтовику не обращается");
    run_checkpointed("digest:d2", &users, &mut |k, to| provider.send(k, to), &mut checkpoints).unwrap();
    assert!(users.iter().all(|u| provider.received.get(u) == Some(&2)), "завтрашний дайджест это другая задача: каждому второе письмо");
}

#[test]
fn a_crash_between_the_send_and_the_checkpoint_does_not_double_the_letter() {
    let users: Vec<String> = (1..=4).map(|i| format!("u{i}")).collect();
    let mut checkpoints = Checkpoints::default();
    let mut provider = Provider { fail_on_call: Some(2), fail_after_send: true, ..Default::default() };
    assert!(run_checkpointed("digest:d", &users, &mut |k, to| provider.send(k, to), &mut checkpoints).is_err(), "ошибка пришла уже после того, как письмо ушло");
    assert!(run_checkpointed("digest:d", &users, &mut |k, to| provider.send(k, to), &mut checkpoints).is_ok(), "повторный запуск прошёл");
    assert!(
        users.iter().all(|u| provider.received.get(u) == Some(&1)),
        "письмо, о котором процесс не успел записать, не должно стать двойным: нужен ключ повтора у самой отправки: {:?}",
        provider.received
    );
}
