use std::collections::HashSet;
use std::sync::Arc;

use api_gateway::{Registry, SHARDS};
use contracts::{Push, UserId};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_session_map_is_sharded_and_stays_consistent() {
    let registry = Arc::new(Registry::default());
    let mut shards = HashSet::new();
    for user in 1..=64u64 {
        shards.insert(Registry::shard_of(UserId(user)));
    }
    assert_eq!(
        shards.len(),
        SHARDS,
        "пользователи расходятся по всем шардам"
    );
    let mut tasks = Vec::new();
    for worker in 0..8u64 {
        let registry = Arc::clone(&registry);
        tasks.push(tokio::spawn(async move {
            let mut kept = Vec::new();
            for n in 0..100u64 {
                let user = UserId(worker * 1_000 + n % 25 + 1);
                let (outbox, inbox) = registry.outbox(8);
                kept.push((registry.enter(user, "", outbox), inbox));
            }
            kept
        }));
    }
    let mut held = Vec::new();
    for task in tasks {
        held.extend(task.await.expect("регистрация"));
    }
    assert_eq!(registry.connections(), 800);
    assert_eq!(registry.users(), 200);
    assert_eq!(registry.seats().len(), 800);
    let user = UserId(1);
    let delivered = registry.deliver(
        user,
        &Push::Followed {
            to: user,
            by: UserId(2),
        },
    );
    assert_eq!(
        delivered,
        registry.devices(user),
        "пуш доходит до всех устройств одного шарда"
    );
    assert_eq!(
        registry.deliver(
            UserId(999_999),
            &Push::Followed {
                to: UserId(999_999),
                by: user
            }
        ),
        0,
        "некому доставить: и кодировать нечего"
    );
    held.truncate(400);
    assert_eq!(
        registry.connections(),
        400,
        "Drop регистрации освобождает место в своём шарде"
    );
    drop(held);
    assert_eq!((registry.connections(), registry.users()), (0, 0));
}
