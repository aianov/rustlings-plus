use std::any::{Any, TypeId};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

pub trait Notifier: Send + Sync {
    fn notify(&self, user: &str, text: &str);
}

pub trait Layer: Send + Sync {
    fn before(&self, command: &str, args: &str) -> Option<Response>;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        todo!()
    }
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        todo!()
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> Response {
        todo!()
    }
}

impl IntoResponse for (u16, String) {
    fn into_response(self) -> Response {
        todo!()
    }
}

impl<T: IntoResponse, E: IntoResponse> IntoResponse for Result<T, E> {
    fn into_response(self) -> Response {
        todo!()
    }
}

#[derive(Default)]
pub struct Context {
    ext: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Context {
    pub fn new() -> Context {
        Context::default()
    }

    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) -> Option<T> {
        todo!("положить значение, вернуть прежнее того же типа")
    }

    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        todo!()
    }

    pub fn get_mut<T: Any + Send + Sync>(&mut self) -> Option<&mut T> {
        todo!()
    }
}

pub struct Bus {}

impl Bus {
    pub fn new() -> Bus {
        todo!()
    }

    pub fn register<F, R>(&mut self, name: &str, handler: F) -> bool
    where
        F: Fn(&mut Context, &str) -> R + Send + Sync + 'static,
        R: IntoResponse + 'static,
    {
        todo!("true, если команда с таким именем уже была и её заменили")
    }

    pub fn layer(&mut self, layer: impl Layer + 'static) {
        todo!("запомнить слой; слои работают до поиска обработчика, в порядке добавления")
    }

    pub fn dispatch(&self, name: &str, ctx: &mut Context, args: &str) -> Response {
        todo!("вызвать обработчик; для неизвестной команды 404 и `unknown command: <имя>`")
    }

    pub fn names(&self) -> Vec<String> {
        todo!("имена команд по алфавиту")
    }
}
