// В шлюзе счётчик подписчиков темы иногда расходится с правдой: тема показывает одного подписчика, а на деле никого.
// Примеры из головы все проходят, потому что редкая последовательность действий в голову не приходит. Такое ловят тестом
// против модели: простая заведомо верная реализация (`Model`) и тысячи случайных последовательностей, и после каждого шага
// обе должны отвечать одинаково. Цель в два шага. Сначала `random_ops(seed, len)`: такой генератор, чтобы повторные
// подписки и отписки без подписки встречались часто (мало пользователей и тем, одно зерно даёт одну и ту же последовательность).
// Потом, когда тест против модели покажет минимальный провал, починить `Registry`.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::{BTreeSet, HashMap};

fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Subscribe(u64, String),
    Unsubscribe(u64, String),
}

pub const USERS: u64 = 4;
pub const TOPICS: usize = 3;

pub fn topic_name(i: usize) -> String {
    format!("topic{i}")
}

#[derive(Default)]
pub struct Registry {
    topics: HashMap<String, Vec<u64>>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    pub fn subscribe(&mut self, user: u64, topic: &str) {
        self.topics.entry(topic.to_string()).or_default().push(user);
    }

    pub fn unsubscribe(&mut self, user: u64, topic: &str) {
        if let Some(users) = self.topics.get_mut(topic) {
            if let Some(at) = users.iter().position(|u| *u == user) {
                users.remove(at);
            }
        }
    }

    pub fn count(&self, topic: &str) -> usize {
        self.topics.get(topic).map_or(0, Vec::len)
    }

    pub fn topics_of(&self, user: u64) -> Vec<String> {
        let mut found: Vec<String> = self.topics.iter().filter(|(_, users)| users.contains(&user)).map(|(topic, _)| topic.clone()).collect();
        found.sort();
        found
    }
}

#[derive(Default)]
pub struct Model {
    pairs: BTreeSet<(u64, String)>,
}

impl Model {
    fn apply(&mut self, op: &Op) {
        match op {
            Op::Subscribe(user, topic) => {
                self.pairs.insert((*user, topic.clone()));
            }
            Op::Unsubscribe(user, topic) => {
                self.pairs.remove(&(*user, topic.clone()));
            }
        }
    }

    fn count(&self, topic: &str) -> usize {
        self.pairs.iter().filter(|(_, t)| t == topic).count()
    }

    fn topics_of(&self, user: u64) -> Vec<String> {
        self.pairs.iter().filter(|(u, _)| *u == user).map(|(_, t)| t.clone()).collect()
    }
}

pub fn run_both(ops: &[Op]) -> Result<(), String> {
    let (mut registry, mut model) = (Registry::new(), Model::default());
    for (step, op) in ops.iter().enumerate() {
        match op {
            Op::Subscribe(user, topic) => registry.subscribe(*user, topic),
            Op::Unsubscribe(user, topic) => registry.unsubscribe(*user, topic),
        }
        model.apply(op);
        for i in 0..TOPICS {
            let topic = topic_name(i);
            let (got, want) = (registry.count(&topic), model.count(&topic));
            if got != want {
                return Err(format!("после шага {} ({op:?}) в теме {topic} подписчиков {got}, а должно быть {want}", step + 1));
            }
        }
        for user in 0..USERS {
            let (got, want) = (registry.topics_of(user), model.topics_of(user));
            if got != want {
                return Err(format!("после шага {} ({op:?}) у пользователя {user} темы {got:?}, а должны быть {want:?}", step + 1));
            }
        }
    }
    Ok(())
}

pub fn shrink(ops: &[Op]) -> Vec<Op> {
    let mut current = ops.to_vec();
    let mut i = 0;
    while i < current.len() {
        let mut shorter = current.clone();
        shorter.remove(i);
        if run_both(&shorter).is_err() {
            current = shorter;
        } else {
            i += 1;
        }
    }
    current
}

pub fn random_ops(seed: u64, len: usize) -> Vec<Op> {
    let _ = (StdRng::seed_from_u64(seed).gen_range(0..1), len);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_agrees_with_the_model_on_random_sequences() {
        for seed in 0..500 {
            let ops = random_ops(seed, 30);
            if let Err(why) = run_both(&ops) {
                let small = shrink(&ops);
                panic!("зерно {seed}: {why}\nминимальная последовательность, на которой расхождение остаётся: {small:?}");
            }
        }
    }

    #[test]
    fn the_generator_is_repeatable() {
        assert_eq!(random_ops(7, 30), random_ops(7, 30), "одно зерно даёт одну последовательность: иначе найденный провал не воспроизвести");
        assert_ne!(random_ops(7, 30), random_ops(8, 30), "разные зёрна дают разное");
        assert_eq!(random_ops(1, 17).len(), 17, "длина та, что просили");
        assert!(random_ops(1, 0).is_empty(), "нулевая длина это пустая последовательность");
    }

    #[test]
    fn the_generator_reaches_the_interesting_cases() {
        let mut saw_subscribe = false;
        let mut saw_unsubscribe = false;
        let mut saw_double_subscribe = false;
        let mut saw_blind_unsubscribe = false;
        let mut saw_resubscribe_after_leave = false;
        for seed in 0..200 {
            let ops = random_ops(seed, 30);
            let mut subscribed: BTreeSet<(u64, String)> = BTreeSet::new();
            let mut left: BTreeSet<(u64, String)> = BTreeSet::new();
            let mut pairs = BTreeSet::new();
            for op in &ops {
                match op {
                    Op::Subscribe(u, t) => {
                        saw_subscribe = true;
                        saw_double_subscribe |= subscribed.contains(&(*u, t.clone()));
                        saw_resubscribe_after_leave |= left.contains(&(*u, t.clone()));
                        subscribed.insert((*u, t.clone()));
                        pairs.insert((*u, t.clone()));
                    }
                    Op::Unsubscribe(u, t) => {
                        saw_unsubscribe = true;
                        saw_blind_unsubscribe |= !subscribed.contains(&(*u, t.clone()));
                        subscribed.remove(&(*u, t.clone()));
                        left.insert((*u, t.clone()));
                        pairs.insert((*u, t.clone()));
                    }
                }
            }
            assert!(pairs.len() <= (USERS as usize) * TOPICS, "в последовательности встретилась пара (пользователь, тема) вне заданных {USERS} пользователей и {TOPICS} тем");
        }
        assert!(saw_subscribe && saw_unsubscribe, "в последовательностях должны быть и подписки, и отписки");
        assert!(saw_double_subscribe, "нет ни одной повторной подписки одного пользователя на ту же тему: именно такие случаи ломают счётчики");
        assert!(saw_blind_unsubscribe, "нет ни одной отписки без подписки");
        assert!(saw_resubscribe_after_leave, "нет ни одной подписки после отписки");
    }

    #[test]
    fn a_plain_scenario_works() {
        let mut registry = Registry::new();
        registry.subscribe(1, "a");
        registry.subscribe(2, "a");
        registry.subscribe(1, "b");
        assert_eq!((registry.count("a"), registry.count("b"), registry.count("c")), (2, 1, 0), "обычные подписки считаются");
        registry.unsubscribe(1, "a");
        assert_eq!(registry.topics_of(1), vec!["b".to_string()], "темы пользователя после отписки");
    }
}
