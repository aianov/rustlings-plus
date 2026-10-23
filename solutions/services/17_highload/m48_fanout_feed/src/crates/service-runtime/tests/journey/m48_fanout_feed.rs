use std::collections::HashSet;

use service_runtime::{EPOCH_MS, IdParts, Lru, SortableIds, compose, split};

#[test]
fn ids_sort_by_time_then_node_then_counter() {
    let ids = SortableIds::new(7);
    let base = EPOCH_MS + 1_000_000;
    let a = ids.next_at(base);
    let b = ids.next_at(base);
    let c = ids.next_at(base + 1);
    assert!(a < b && b < c, "{a} {b} {c}");
    assert_eq!(
        split(b),
        IdParts {
            millis: base,
            node: 7,
            counter: 1
        }
    );
    assert_eq!(compose(split(c)), c);
    let other = SortableIds::new(8).next_at(base);
    assert_ne!(other, a, "узлы в одну миллисекунду не совпадают");
}

#[test]
fn clock_rollback_and_overflow_never_repeat() {
    let ids = SortableIds::new(1);
    let base = EPOCH_MS + 5_000;
    let mut seen = HashSet::new();
    let mut last = 0;
    for step in 0..10_000u64 {
        let now = if step % 100 == 0 { base - 50 } else { base };
        let id = ids.next_at(now);
        assert!(id > last, "id растут даже при откате часов: шаг {step}");
        assert!(seen.insert(id));
        last = id;
    }
    assert!(
        split(last).millis > base,
        "переполнение счётчика сдвигает миллисекунду"
    );
}

#[test]
fn observe_continues_after_restart() {
    let before = SortableIds::new(1);
    let newest = before.next_at(EPOCH_MS + 10_000);
    let after = SortableIds::new(1);
    after.observe(newest);
    assert!(
        after.next_at(EPOCH_MS + 9_000) > newest,
        "после перезапуска не раньше последнего"
    );
}

#[test]
fn lru_evicts_the_least_recently_used_in_constant_time() {
    let mut lru = Lru::new(3);
    assert_eq!(lru.put("a", 1), None);
    assert_eq!(lru.put("b", 2), None);
    assert_eq!(lru.put("c", 3), None);
    assert_eq!(lru.get(&"a"), Some(&1), "a снова свежий");
    assert_eq!(lru.put("d", 4), Some("b"), "вытеснен самый давний: b");
    assert_eq!(lru.keys_newest_first(), ["d", "a", "c"]);
    assert_eq!(lru.put("a", 10), None, "перезапись не вытесняет");
    assert_eq!(lru.peek(&"a"), Some(&10));
    assert!(lru.remove(&"c"));
    assert!(!lru.remove(&"c"));
    assert_eq!(lru.put("e", 5), None, "место освободилось удалением");
    assert_eq!(lru.len(), 3);
    for n in 0..10_000 {
        let _ = lru.put(if n % 2 == 0 { "x" } else { "y" }, n);
    }
    assert_eq!(lru.len(), 3);
}
