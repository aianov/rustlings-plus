/// То, что умеет запускать и останавливать экземпляры: настоящие процессы или контейнеры, а в тестах подмена.
#[allow(async_fn_in_trait)]
pub trait Backend {
    async fn start(&self, instance: &str);
    /// Ответил ли экземпляр на `/ready`.
    async fn ready(&self, instance: &str) -> bool;
    /// SIGTERM.
    async fn term(&self, instance: &str);
    async fn exited(&self, instance: &str) -> bool;
    /// SIGKILL.
    async fn kill(&self, instance: &str);
    /// Сколько запросов экземпляр ведёт прямо сейчас.
    async fn in_flight(&self, instance: &str) -> usize;
}
