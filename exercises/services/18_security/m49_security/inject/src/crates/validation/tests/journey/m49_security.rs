use std::time::{Duration, Instant};

use validation::Pattern;

#[test]
fn a_hostile_pattern_does_not_explode() {
    let pattern = Pattern::parse("a*a*a*a*a*a*a*a*a*a*a*a*b").expect("шаблон");
    let text = "a".repeat(5_000);
    let started = Instant::now();
    assert!(!pattern.is_match(&text));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "запоминание неудач держит перебор полиномиальным: {:?}",
        started.elapsed()
    );
    let nested = Pattern::parse("(a?){30}a{30}").ok();
    if let Some(nested) = nested {
        let text = "a".repeat(30);
        let started = Instant::now();
        let _ = nested.is_match(&text);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
