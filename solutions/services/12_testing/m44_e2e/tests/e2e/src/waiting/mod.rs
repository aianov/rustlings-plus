use std::fmt::Debug;
use std::future::Future;

pub async fn eventually<T, E, F, Fut>(what: &str, check: F) -> T
where
    E: Debug,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    test_support::eventually(what, check)
        .await
        .unwrap_or_else(|gave_up| panic!("{gave_up}"))
}
