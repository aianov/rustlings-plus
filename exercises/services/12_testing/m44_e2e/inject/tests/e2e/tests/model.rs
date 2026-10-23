use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use contracts::{Origin, UserId, Username};
use test_support::{Rng, seed, unique_name};
use user_service::{MemoryAvatars, MemoryUsers, UserError, UserService};

#[derive(Default)]
struct Model {
    follows: BTreeSet<(UserId, UserId)>,
    mutes: BTreeSet<(UserId, UserId)>,
}

impl Model {
    fn follow(&mut self, who: UserId, whom: UserId) -> bool {
        self.follows.insert((who, whom))
    }

    fn mute(&mut self, who: UserId, whom: UserId) -> bool {
        self.mutes.insert((who, whom))
    }

    fn following(&self, id: UserId) -> BTreeSet<UserId> {
        self.follows
            .iter()
            .filter(|(who, _)| *who == id)
            .map(|(_, whom)| *whom)
            .collect()
    }

    fn followers(&self, id: UserId) -> BTreeSet<UserId> {
        self.follows
            .iter()
            .filter(|(_, whom)| *whom == id)
            .map(|(who, _)| *who)
            .collect()
    }

    fn muted(&self, id: UserId) -> BTreeSet<UserId> {
        self.mutes
            .iter()
            .filter(|(who, _)| *who == id)
            .map(|(_, whom)| *whom)
            .collect()
    }

    fn audience(&self, id: UserId) -> BTreeSet<UserId> {
        self.followers(id)
            .into_iter()
            .filter(|follower| !self.mutes.contains(&(*follower, id)))
            .collect()
    }
}

fn set(ids: &[UserId]) -> BTreeSet<UserId> {
    ids.iter().copied().collect()
}

#[tokio::test]
async fn subscriptions_match_a_simple_model_on_random_sequences() {
    let seed = seed();
    let origin = Origin::new("e2e-model");
    for round in 0..20u64 {
        let mut rng = Rng::seeded(seed.wrapping_add(round));
        let service = UserService::new(
            Arc::new(MemoryUsers::new()),
            Arc::new(MemoryAvatars::default()),
        );
        let mut users = Vec::new();
        for _ in 0..6 {
            let name = Username::try_from(unique_name("m").as_str()).expect("ник");
            users.push(
                service
                    .register(name, &origin)
                    .await
                    .expect("регистрация")
                    .id,
            );
        }
        let ghost = UserId(999_999);
        let mut model = Model::default();
        let mut log = Vec::new();
        for _ in 0..60 {
            let who = users[rng.index(users.len())];
            let whom = if rng.chance(5) {
                ghost
            } else {
                users[rng.index(users.len())]
            };
            let muting = rng.chance(30);
            log.push(format!(
                "{} {who} {whom}",
                if muting { "mute" } else { "follow" }
            ));
            let got = if muting {
                service.mute(who, whom, &origin).await
            } else {
                service.follow(who, whom, &origin).await
            };
            let context = || format!("seed {seed}, раунд {round}, шаги:\n{}", log.join("\n"));
            match got {
                Err(UserError::SelfFollow | UserError::SelfMute) => {
                    assert_eq!(who, whom, "{}", context());
                }
                Err(UserError::UnknownUser(..)) => {
                    assert_eq!(whom, ghost, "{}", context());
                }
                Err(other) => panic!("неожиданная ошибка {other}: {}", context()),
                Ok(changed) => {
                    let expected = if muting {
                        model.mute(who, whom)
                    } else {
                        model.follow(who, whom)
                    };
                    assert_eq!(
                        changed,
                        expected,
                        "changed разошёлся с моделью: {}",
                        context()
                    );
                }
            }
        }
        let mut seen = BTreeMap::new();
        for id in &users {
            let relations = service.relations(*id).await.expect("связи");
            assert_eq!(
                set(&relations.following),
                model.following(*id),
                "seed {seed}"
            );
            assert_eq!(
                set(&relations.followers),
                model.followers(*id),
                "seed {seed}"
            );
            assert_eq!(set(&relations.muted), model.muted(*id), "seed {seed}");
            assert_eq!(set(&relations.audience), model.audience(*id), "seed {seed}");
            seen.insert(*id, relations.followers.len());
        }
        assert_eq!(
            seen.values().sum::<usize>(),
            model.follows.len(),
            "seed {seed}: каждая подписка видна ровно у одного автора"
        );
    }
}
