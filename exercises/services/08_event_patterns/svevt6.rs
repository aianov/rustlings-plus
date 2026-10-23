// Счётчик лайков защищён от повторной доставки, и всё работало, пока пользователь не жаловался: «поставил лайк,
// снял, поставил снова, а счётчик показывает ноль». При этом повторы событий, пришедшие с опозданием, по-прежнему
// не должны менять счёт, даже если лайк с тех пор успели снять. Цель: чтобы работало и то, и другое.

use std::collections::{HashMap, HashSet};

fn main() {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Like,
    Unlike,
}

#[derive(Debug, Clone)]
pub struct Reaction {
    pub event_id: String,
    pub post_id: String,
    pub user_id: String,
    pub action: Action,
}

#[derive(Default)]
pub struct Counter {
    seen: HashSet<(Action, String, String)>,
    likes: HashMap<String, i64>,
}

impl Counter {
    pub fn new() -> Counter {
        Counter::default()
    }

    pub fn on_reaction(&mut self, reaction: &Reaction) {
        let key = (reaction.action, reaction.post_id.clone(), reaction.user_id.clone());
        if !self.seen.insert(key) {
            return;
        }
        let delta = match reaction.action {
            Action::Like => 1,
            Action::Unlike => -1,
        };
        *self.likes.entry(reaction.post_id.clone()).or_default() += delta;
    }

    pub fn count(&self, post_id: &str) -> i64 {
        self.likes.get(post_id).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reaction(id: &str, user: &str, action: Action) -> Reaction {
        Reaction { event_id: id.into(), post_id: "p1".into(), user_id: user.into(), action }
    }

    #[test]
    fn like_unlike_like_leaves_one_like() {
        let mut counter = Counter::new();
        counter.on_reaction(&reaction("e1", "bob", Action::Like));
        counter.on_reaction(&reaction("e2", "bob", Action::Unlike));
        assert_eq!(counter.count("p1"), 0, "лайк снят");
        counter.on_reaction(&reaction("e3", "bob", Action::Like));
        assert_eq!(counter.count("p1"), 1, "второй лайк того же человека, это новое событие, а не повтор");
    }

    #[test]
    fn redelivery_after_the_state_changed_changes_nothing() {
        let mut counter = Counter::new();
        counter.on_reaction(&reaction("e1", "bob", Action::Like));
        counter.on_reaction(&reaction("e2", "bob", Action::Unlike));
        counter.on_reaction(&reaction("e1", "bob", Action::Like));
        assert_eq!(counter.count("p1"), 0, "опоздавший повтор первого лайка не должен вернуть лайк");
        counter.on_reaction(&reaction("e3", "bob", Action::Like));
        counter.on_reaction(&reaction("e2", "bob", Action::Unlike));
        counter.on_reaction(&reaction("e3", "bob", Action::Like));
        assert_eq!(counter.count("p1"), 1, "повторы e2 и e3 тоже ничего не меняют");
    }

    #[test]
    fn different_people_are_independent() {
        let mut counter = Counter::new();
        for (i, user) in ["ann", "bob", "cat"].iter().enumerate() {
            counter.on_reaction(&reaction(&format!("e{i}"), user, Action::Like));
        }
        assert_eq!(counter.count("p1"), 3, "три человека, три лайка");
    }

    #[test]
    fn every_event_is_applied_exactly_once_under_heavy_redelivery() {
        let mut counter = Counter::new();
        let events: Vec<Reaction> = (0..20)
            .map(|i| reaction(&format!("e{i}"), "bob", if i % 2 == 0 { Action::Like } else { Action::Unlike }))
            .collect();
        for _ in 0..3 {
            for event in &events {
                counter.on_reaction(event);
            }
        }
        assert_eq!(counter.count("p1"), 0, "десять лайков и десять снятий, сколько бы раз их ни доставили");
    }
}
