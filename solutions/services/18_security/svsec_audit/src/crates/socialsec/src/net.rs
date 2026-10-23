use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body_len: usize,
    pub title: Option<String>,
}

/// Сеть, из которой сервис достаёт страницы для превью.
pub trait Net: Send + Sync {
    fn resolve(&self, host: &str) -> Vec<IpAddr>;
    /// Клиент сам превращает имя в адрес (ещё один запрос DNS) и соединяется.
    fn get_by_name(&self, host: &str, port: u16, https: bool, path: &str) -> Response;
    /// Соединение с конкретным адресом.
    fn get(&self, addr: IpAddr, host: &str, port: u16, https: bool, path: &str) -> Response;
}

/// Хешер паролей. Настоящий медленный, в тестах подмена со счётчиком.
pub trait Hasher: Send + Sync {
    fn hash(&self, password: &str) -> String;
    fn verify(&self, hash: &str, password: &str) -> bool;
}
