// Корзину нужно напомнить через час, неоплаченный заказ закрыть через полчаса. В задаче есть срок начала `run_at`
// (в миллисекундах), а JetStream выдаёт её сразу. Цель: `tick` берёт задачу, а если срок ещё не наступил,
// возвращает её в очередь так, чтобы она вернулась ровно когда пора, и не вызывает обработчик; остальные
// задачи в это время работают без помех. Готовая задача выполняется как обычно: успех, повтор или мёртвая очередь.

fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub run_at: u64,
}

impl Job {
    pub fn new(id: &str, run_at: u64) -> Job {
        Job { id: id.to_string(), kind: "send".to_string(), run_at }
    }
}

#[derive(Debug, Clone)]
pub struct Delivery {
    pub job: Job,
    pub delivered: u32,
}

struct Slot {
    job: Job,
    delivered: u32,
    ready_at: u64,
    leased_until: u64,
}

pub struct MemQueue {
    ack_wait: u64,
    slots: Vec<Slot>,
    dead: Vec<(Job, String)>,
}

impl MemQueue {
    pub fn new(ack_wait: u64) -> MemQueue {
        MemQueue { ack_wait, slots: Vec::new(), dead: Vec::new() }
    }

    pub fn enqueue(&mut self, job: Job) {
        self.slots.push(Slot { job, delivered: 0, ready_at: 0, leased_until: 0 });
    }

    pub fn take(&mut self, now: u64) -> Option<Delivery> {
        let slot = self.slots.iter_mut().find(|s| s.ready_at <= now && s.leased_until <= now)?;
        slot.delivered += 1;
        slot.leased_until = now + self.ack_wait;
        Some(Delivery { job: slot.job.clone(), delivered: slot.delivered })
    }

    pub fn ack(&mut self, id: &str) {
        self.slots.retain(|s| s.job.id != id);
    }

    pub fn nak(&mut self, id: &str, delay: u64, now: u64) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.job.id == id) {
            slot.ready_at = now + delay;
            slot.leased_until = 0;
        }
    }

    pub fn term(&mut self, id: &str, reason: &str) {
        if let Some(i) = self.slots.iter().position(|s| s.job.id == id) {
            let slot = self.slots.remove(i);
            self.dead.push((slot.job, reason.to_string()));
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn dead(&self) -> &[(Job, String)] {
        &self.dead
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobError {
    Temporary(String),
    Permanent(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Done,
    Retry { after_ms: u64 },
    Dead { reason: String },
}

pub const MAX_ATTEMPTS: u32 = 5;

pub fn backoff_ms(attempt: u32) -> u64 {
    let exp = attempt.saturating_sub(1).min(10);
    (1000u64 << exp).min(60_000)
}

pub fn decide(result: Result<(), JobError>, attempt: u32) -> Action {
    match result {
        Ok(()) => Action::Done,
        Err(JobError::Permanent(why)) => Action::Dead { reason: why },
        Err(JobError::Temporary(why)) if attempt >= MAX_ATTEMPTS => Action::Dead { reason: format!("попытки исчерпаны ({MAX_ATTEMPTS}): {why}") },
        Err(JobError::Temporary(_)) => Action::Retry { after_ms: backoff_ms(attempt) },
    }
}

pub struct Worker {
    pub queue: MemQueue,
}

impl Worker {
    pub fn tick(&mut self, now: u64, handler: &mut impl FnMut(&Delivery) -> Result<(), JobError>) -> bool {
        let _ = (now, handler);
        let delivery = self.queue.take(now);
        let _ = delivery;
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(worker: &mut Worker, handler: &mut impl FnMut(&Delivery) -> Result<(), JobError>, from: u64, until: u64) {
        let mut now = from;
        while now <= until {
            while worker.tick(now, handler) {}
            now += 100;
        }
    }

    #[test]
    fn a_delayed_job_does_not_run_early_and_runs_when_due() {
        let mut queue = MemQueue::new(30_000);
        queue.enqueue(Job::new("reminder", 5000));
        let mut worker = Worker { queue };
        let mut ran_at = Vec::new();
        let mut now = 0;
        while now <= 8000 {
            let mut handler = |_: &Delivery| {
                ran_at.push(now);
                Ok(())
            };
            worker.tick(now, &mut handler);
            now += 100;
        }
        assert_eq!(ran_at, vec![5000], "обработчик вызван один раз и не раньше срока: {ran_at:?}");
        assert!(worker.queue.is_empty(), "после выполнения очередь пуста");
    }

    #[test]
    fn a_waiting_job_does_not_hold_up_the_ready_ones() {
        let mut queue = MemQueue::new(30_000);
        queue.enqueue(Job::new("later", 10_000));
        queue.enqueue(Job::new("now", 0));
        let mut worker = Worker { queue };
        let mut order = Vec::new();
        run(&mut worker, &mut |d| { order.push((d.job.id.clone(), d.job.run_at)); Ok(()) }, 0, 100);
        assert_eq!(order, vec![("now".to_string(), 0)], "готовая задача выполнена сразу, хотя отложенная стояла перед ней: {order:?}");
        run(&mut worker, &mut |d| { order.push((d.job.id.clone(), d.job.run_at)); Ok(()) }, 200, 10_500);
        assert_eq!(order.len(), 2, "а отложенная дождалась своего часа");
        assert_eq!(order[1].0, "later", "отложенная идёт второй");
    }

    #[test]
    fn waiting_is_one_return_to_the_queue_not_polling() {
        let mut queue = MemQueue::new(30_000);
        queue.enqueue(Job::new("in-a-minute", 90_000));
        let mut worker = Worker { queue };
        let mut seen = Vec::new();
        run(&mut worker, &mut |d| { seen.push(d.delivered); Ok(()) }, 0, 95_000);
        assert_eq!(seen.len(), 1, "обработчик вызван один раз");
        assert!(seen[0] <= 2, "к моменту выполнения задачу выдавали {} раз: ожидание должно быть одним возвратом на всё оставшееся время, а не опросом", seen[0]);
    }

    #[test]
    fn a_due_job_runs_at_once() {
        let mut queue = MemQueue::new(30_000);
        queue.enqueue(Job::new("due", 700));
        let mut worker = Worker { queue };
        let mut calls = 0;
        worker.tick(1000, &mut |_| { calls += 1; Ok(()) });
        assert_eq!(calls, 1, "срок прошёл раньше, чем пришли за задачей: выполняем без ожидания");
        assert!(worker.queue.is_empty(), "и подтверждаем");
    }

    #[test]
    fn a_delayed_job_that_fails_is_still_retried_by_the_usual_rules() {
        let mut queue = MemQueue::new(30_000);
        queue.enqueue(Job::new("flaky", 2000));
        let mut worker = Worker { queue };
        let mut calls = 0;
        run(&mut worker, &mut |_| { calls += 1; if calls == 1 { Err(JobError::Temporary("моргнуло".to_string())) } else { Ok(()) } }, 0, 20_000);
        assert_eq!(calls, 2, "первая попытка не удалась, вторая прошла");
        assert!(worker.queue.dead().is_empty() && worker.queue.is_empty(), "задача выполнена и в мёртвую очередь не попала");
    }
}
