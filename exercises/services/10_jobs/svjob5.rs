// Дежурный пожаловался: очередь забита. Пользователь удалил аккаунт, а задача «прислать ему письмо» повторяется
// каждые минуту-две, и так будет вечно (то есть до тех пор, пока кто-нибудь не заметит). Хуже того, задачи,
// которые честно исчерпали попытки, просто пропадают, и никто не знает, что с ними случилось. Цель: воркер
// повторяет только то, что может пройти позже; безнадёжные задачи сразу уходят в мёртвую очередь с понятной
// причиной, как и исчерпавшие попытки. Неправильных мест в воркере два.

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

#[derive(Debug, Clone, PartialEq)]
pub enum HandlerError {
    Timeout,
    Unavailable(String),
    UserNotFound(String),
    BadPayload(String),
}

fn classify(error: HandlerError) -> JobError {
    match error {
        HandlerError::Timeout => JobError::Temporary("таймаут".to_string()),
        HandlerError::Unavailable(who) => JobError::Temporary(format!("{who} недоступен")),
        HandlerError::UserNotFound(user) => JobError::Temporary(format!("пользователь {user} не найден")),
        HandlerError::BadPayload(why) => JobError::Temporary(format!("нечитаемая задача: {why}")),
    }
}

pub struct Worker {
    pub queue: MemQueue,
}

impl Worker {
    pub fn tick(&mut self, now: u64, handler: &mut impl FnMut(&Job) -> Result<(), HandlerError>) -> bool {
        let Some(delivery) = self.queue.take(now) else { return false };
        let result = handler(&delivery.job).map_err(classify);
        match decide(result, delivery.delivered) {
            Action::Done => self.queue.ack(&delivery.job.id),
            Action::Retry { after_ms } => self.queue.nak(&delivery.job.id, after_ms, now),
            Action::Dead { .. } => self.queue.ack(&delivery.job.id),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(worker: &mut Worker, handler: &mut impl FnMut(&Job) -> Result<(), HandlerError>, until: u64) {
        let mut now = 0;
        while now <= until {
            while worker.tick(now, handler) {}
            now += 100;
        }
    }

    fn worker_with(id: &str) -> Worker {
        let mut queue = MemQueue::new(30_000);
        queue.enqueue(Job::new(id, 0));
        Worker { queue }
    }

    #[test]
    fn a_deleted_user_is_tried_once_and_reported() {
        let mut worker = worker_with("mail:user-42");
        let mut calls = 0;
        run(&mut worker, &mut |_| { calls += 1; Err(HandlerError::UserNotFound("user-42".to_string())) }, 600_000);
        assert_eq!(calls, 1, "пользователя нет, и повторы ничего не изменят: задача пробуется один раз, а не {calls}");
        assert_eq!(worker.queue.dead().len(), 1, "задача уходит в мёртвую очередь, а не пропадает");
        assert!(worker.queue.dead()[0].1.contains("user-42"), "в причине названо, чей пользователь не найден: {}", worker.queue.dead()[0].1);
        assert!(worker.queue.is_empty(), "и в рабочей очереди её больше нет");
    }

    #[test]
    fn a_broken_payload_is_not_retried_either() {
        let mut worker = worker_with("mail:broken");
        let mut calls = 0;
        run(&mut worker, &mut |_| { calls += 1; Err(HandlerError::BadPayload("нет поля order".to_string())) }, 600_000);
        assert_eq!(calls, 1, "кривой конверт не станет верным от повторов");
        assert_eq!(worker.queue.dead().len(), 1, "в мёртвую очередь");
    }

    #[test]
    fn a_timeout_is_retried_with_growing_pauses_until_it_works() {
        let mut worker = worker_with("mail:slow");
        let mut times = Vec::new();
        run(
            &mut worker,
            &mut |_| {
                times.push(times.len() as u64);
                if times.len() <= 3 { Err(HandlerError::Timeout) } else { Ok(()) }
            },
            60_000,
        );
        assert_eq!(times.len(), 4, "три неудачи и успех");
        assert!(worker.queue.is_empty() && worker.queue.dead().is_empty(), "всё выполнено, мёртвая очередь пуста");
    }

    #[test]
    fn the_pauses_between_retries_grow() {
        let mut worker = worker_with("mail:slow");
        let mut at = Vec::new();
        let mut now = 0;
        while now <= 60_000 && !worker.queue.is_empty() {
            let mut handler = |_: &Job| Err(HandlerError::Timeout);
            if worker.tick(now, &mut handler) {
                at.push(now);
            }
            now += 100;
        }
        assert_eq!(at, vec![0, 1000, 3000, 7000, 15_000], "попытки в моменты 0, +1 с, +2 с, +4 с, +8 с");
    }

    #[test]
    fn exhausted_attempts_are_reported_not_swallowed() {
        let mut worker = worker_with("mail:never");
        let mut calls = 0;
        run(&mut worker, &mut |_| { calls += 1; Err(HandlerError::Unavailable("smtp".to_string())) }, 600_000);
        assert_eq!(calls, MAX_ATTEMPTS, "ровно столько попыток, сколько разрешено");
        assert_eq!(worker.queue.dead().len(), 1, "после последней неудачи задача лежит в мёртвой очереди, а не исчезла бесследно");
        assert!(worker.queue.dead()[0].1.contains("smtp"), "и причина не потеряна: {}", worker.queue.dead()[0].1);
    }
}
