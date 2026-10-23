// Ежедневный дайджест уходит всем пользователям. Рассылка упала на шестом письме (почтовик моргнул), задачу
// запустили заново, и она начала с первого: пять человек получили по два дайджеста. Цель: повторный запуск
// продолжает с места остановки и не отправляет уже отправленное. Между «письмо ушло» и «запомнили, что ушло»
// процесс тоже может упасть, и письмо не должно стать двойным и в этом случае. Заготовки `Mailer` и
// `Checkpoints` уже есть, провайдер почты отбрасывает повторы с тем же ключом.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn main() {}

#[derive(Default)]
pub struct Mailer {
    effective: Mutex<HashMap<String, u32>>,
    seen_keys: Mutex<Vec<String>>,
    calls: AtomicUsize,
    fail_on_call: Mutex<Option<usize>>,
}

impl Mailer {
    pub fn fail_on_call(&self, n: usize) {
        *self.fail_on_call.lock().unwrap() = Some(n);
    }

    pub fn send(&self, key: Option<&str>, to: &str) -> Result<(), String> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if *self.fail_on_call.lock().unwrap() == Some(call) {
            return Err("почтовик моргнул".to_string());
        }
        if let Some(key) = key {
            let mut seen = self.seen_keys.lock().unwrap();
            if seen.iter().any(|k| k == key) {
                return Ok(());
            }
            seen.push(key.to_string());
        }
        *self.effective.lock().unwrap().entry(to.to_string()).or_default() += 1;
        Ok(())
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn received(&self, user: &str) -> u32 {
        self.effective.lock().unwrap().get(user).copied().unwrap_or(0)
    }
}

#[derive(Default)]
pub struct Checkpoints {
    values: Mutex<HashMap<String, String>>,
    fail_next_set: AtomicBool,
}

impl Checkpoints {
    pub fn get(&self, job_id: &str) -> Option<String> {
        self.values.lock().unwrap().get(job_id).cloned()
    }

    pub fn set(&self, job_id: &str, value: &str) -> Result<(), String> {
        if self.fail_next_set.swap(false, Ordering::SeqCst) {
            return Err("не удалось записать контрольную точку".to_string());
        }
        self.values.lock().unwrap().insert(job_id.to_string(), value.to_string());
        Ok(())
    }

    pub fn fail_next_set(&self) {
        self.fail_next_set.store(true, Ordering::SeqCst);
    }
}

pub fn send_digests(job_id: &str, users: &[String], mailer: &Mailer, checkpoints: &Checkpoints) -> Result<usize, String> {
    let _ = checkpoints.get(job_id);
    let mut sent = 0;
    for user in users {
        mailer.send(None, user)?;
        sent += 1;
    }
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn users(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("u{i:03}")).collect()
    }

    #[test]
    fn without_a_crash_everyone_gets_one_digest() {
        let (mailer, checkpoints) = (Mailer::default(), Checkpoints::default());
        assert_eq!(send_digests("digest:2025-06-01", &users(20), &mailer, &checkpoints), Ok(20), "двадцать писем");
        assert!(users(20).iter().all(|u| mailer.received(u) == 1), "каждый получил по одному");
    }

    #[test]
    fn after_a_crash_the_rerun_continues_and_sends_nothing_twice() {
        let (mailer, checkpoints) = (Mailer::default(), Checkpoints::default());
        let all = users(20);
        mailer.fail_on_call(6);
        assert!(send_digests("digest:2025-06-01", &all, &mailer, &checkpoints).is_err(), "на шестом письме почтовик моргнул");
        let calls_before = mailer.calls();
        assert_eq!(send_digests("digest:2025-06-01", &all, &mailer, &checkpoints), Ok(15), "остаток: пятнадцать писем, начиная с шестого");
        assert!(mailer.calls() - calls_before <= 15, "второй запуск сделал {} обращений к почтовику: начинать с начала нельзя", mailer.calls() - calls_before);
        for u in &all {
            assert_eq!(mailer.received(u), 1, "{u} получил {} дайджестов вместо одного", mailer.received(u));
        }
    }

    #[test]
    fn a_crash_between_the_send_and_the_checkpoint_does_not_double_the_letter() {
        let (mailer, checkpoints) = (Mailer::default(), Checkpoints::default());
        let all = users(10);
        checkpoints.set("digest:d", "u003").unwrap();
        checkpoints.fail_next_set();
        assert!(send_digests("digest:d", &all, &mailer, &checkpoints).is_err(), "запись контрольной точки упала после отправки письма u004");
        assert_eq!(mailer.received("u004"), 1, "письмо u004 успело уйти");
        send_digests("digest:d", &all, &mailer, &checkpoints).expect("повторный запуск");
        assert_eq!(mailer.received("u004"), 1, "u004 заново не получает: точка не записалась, и защита должна быть ещё и в самом письме (ключ повтора)");
        assert!(all.iter().skip(3).all(|u| mailer.received(u) == 1), "остальные по одному");
        assert_eq!(mailer.received("u001"), 0, "а те, что до точки, не получили ничего: их запуск не касался");
    }

    #[test]
    fn a_finished_job_that_is_delivered_again_does_nothing() {
        let (mailer, checkpoints) = (Mailer::default(), Checkpoints::default());
        let all = users(5);
        send_digests("digest:d", &all, &mailer, &checkpoints).unwrap();
        let calls = mailer.calls();
        assert_eq!(send_digests("digest:d", &all, &mailer, &checkpoints), Ok(0), "задача выдана брокером повторно после ack, который потерялся");
        assert_eq!(mailer.calls(), calls, "и ни одного обращения к почтовику");
    }

    #[test]
    fn tomorrows_digest_is_a_different_job_and_starts_from_the_first_user() {
        let (mailer, checkpoints) = (Mailer::default(), Checkpoints::default());
        let all = users(4);
        send_digests("digest:day1", &all, &mailer, &checkpoints).unwrap();
        send_digests("digest:day2", &all, &mailer, &checkpoints).unwrap();
        assert!(all.iter().all(|u| mailer.received(u) == 2), "по одному дайджесту в день: точка и ключ привязаны к заданию, а не к пользователю");
    }
}
