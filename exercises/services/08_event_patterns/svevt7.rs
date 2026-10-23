// notify-service на каждое `post.commented` увеличивает автору поста счётчик непрочитанных и помечает событие
// обработанным. Процесс может упасть где угодно, и тогда брокер доставит событие снова. Цель: после любого сбоя и
// повторной доставки счётчик вырос ровно на единицу: не потерян и не удвоен. Эффект и отметка «обработано» либо
// сохраняются оба, либо ни один. Точка сбоя между ними вызывается так: self.crash_point()?.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

fn main() {}

#[derive(Debug, PartialEq)]
pub struct Crash;

#[derive(Debug, PartialEq)]
pub enum Handled {
    Applied,
    Duplicate,
}

#[derive(Clone, Default)]
struct State {
    processed: HashSet<String>,
    unread: HashMap<String, u32>,
}

#[derive(Default)]
pub struct Store {
    state: Mutex<State>,
    crash_armed: AtomicBool,
}

impl Store {
    pub fn new() -> Store {
        Store::default()
    }

    pub fn crash_once(&self) {
        self.crash_armed.store(true, Ordering::SeqCst);
    }

    fn crash_point(&self) -> Result<(), Crash> {
        if self.crash_armed.swap(false, Ordering::SeqCst) { Err(Crash) } else { Ok(()) }
    }

    pub fn unread(&self, user: &str) -> u32 {
        self.state.lock().unwrap().unread.get(user).copied().unwrap_or(0)
    }

    pub fn handle(&self, event_id: &str, post_author: &str) -> Result<Handled, Crash> {
        let _ = (event_id, post_author);
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_event_is_counted_and_a_repeat_is_not() {
        let store = Store::new();
        assert_eq!(store.handle("e1", "alice"), Ok(Handled::Applied), "первая доставка");
        assert_eq!(store.handle("e1", "alice"), Ok(Handled::Duplicate), "повтор узнан");
        assert_eq!(store.unread("alice"), 1, "счётчик вырос один раз");
    }

    #[test]
    fn a_crash_between_the_effect_and_the_mark_loses_neither() {
        let store = Store::new();
        store.crash_once();
        assert_eq!(store.handle("e1", "alice"), Err(Crash), "точка сбоя должна сработать между эффектом и отметкой");
        assert_eq!(store.handle("e1", "alice"), Ok(Handled::Applied), "брокер доставил событие снова, обработки не было, значит это первый раз");
        assert_eq!(store.unread("alice"), 1, "после сбоя и повтора ровно один: не ноль (потеря) и не два (дубль)");
    }

    #[test]
    fn a_crash_leaves_no_half_applied_effect_behind() {
        let store = Store::new();
        store.crash_once();
        let _ = store.handle("e1", "alice");
        assert_eq!(store.unread("alice"), 0, "упавшая обработка не оставляет эффекта: счётчик нетронут до повторной доставки");
    }

    #[test]
    fn the_crash_is_one_off_and_other_events_are_independent() {
        let store = Store::new();
        store.crash_once();
        assert!(store.handle("e1", "alice").is_err(), "сбой");
        assert_eq!(store.handle("e2", "bob"), Ok(Handled::Applied), "следующее событие обрабатывается нормально");
        assert_eq!(store.handle("e1", "alice"), Ok(Handled::Applied), "повтор упавшего");
        assert_eq!((store.unread("alice"), store.unread("bob")), (1, 1), "у каждого по одному");
    }

    #[test]
    fn several_crashes_in_a_row_still_end_with_exactly_one() {
        let store = Store::new();
        for _ in 0..3 {
            store.crash_once();
            assert!(store.handle("e1", "alice").is_err(), "каждый раз падаем на полпути");
        }
        assert_eq!(store.handle("e1", "alice"), Ok(Handled::Applied), "наконец дошло");
        assert_eq!(store.handle("e1", "alice"), Ok(Handled::Duplicate), "и дальше это повтор");
        assert_eq!(store.unread("alice"), 1, "в итоге один");
    }

    #[test]
    fn eight_parallel_events_for_one_author_all_count() {
        use std::sync::{Arc, Barrier};
        let store = Arc::new(Store::new());
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let (store, barrier) = (store.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    store.handle(&format!("e{i}"), "alice").unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(store.unread("alice"), 8, "восемь разных событий подряд должны дать восемь: обработка целиком под одним замком, а не «клонировал, отпустил, подменил»");
    }

    #[test]
    fn one_event_delivered_by_eight_threads_counts_once() {
        use std::sync::{Arc, Barrier};
        let store = Arc::new(Store::new());
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (store, barrier) = (store.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    store.handle("same", "alice").unwrap()
                })
            })
            .collect();
        let applied = handles.into_iter().map(|h| h.join().unwrap()).filter(|h| *h == Handled::Applied).count();
        assert_eq!(applied, 1, "одно событие применяется ровно один раз даже при параллельной доставке");
        assert_eq!(store.unread("alice"), 1, "и счётчик вырос на единицу");
    }
}
