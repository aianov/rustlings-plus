use std::future::Future;
use std::time::Duration;

pub fn unique_prefix() -> String {
    todo!()
}

pub async fn eventually<T, F, Fut>(within: Duration, every: Duration, probe: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let _ = (within, every, probe);
    todo!()
}
