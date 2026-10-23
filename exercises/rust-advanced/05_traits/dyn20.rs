// Мини-фреймворк маршрутов. Обработчик в таблице это объект трейта `Handler`, а писать удобно обычными
// функциями и замыканиями `Fn(&Request) -> Response`. Мост между ними называется стиранием типа:
// обёртка `FnHandler<F>` реализует `Handler` для любого такого `F`, и в таблицу кладут `Box<dyn Handler>`.
// Обработчики разных типов оказываются рядом, и таблица ничего про них не знает.
// Неизвестный путь даёт ответ со статусом 404 и телом `not found`.
// TODO: Реализуй мост и таблицу маршрутов.

use std::collections::HashMap;

pub struct Request {
    pub path: String,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

pub trait Handler {
    fn call(&self, request: &Request) -> Response;
}

pub struct FnHandler<F>(pub F);

pub struct Routes {
    table: HashMap<String, Box<dyn Handler>>,
}

impl Default for Routes {
    fn default() -> Self {
        Routes::new()
    }
}

impl Routes {
    pub fn new() -> Routes {
        Routes { table: HashMap::new() }
    }

    pub fn add<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(&Request) -> Response + 'static,
    {
        todo!("обернуть в FnHandler и положить в таблицу")
    }

    pub fn dispatch(&self, path: &str) -> Response {
        todo!("найти обработчик и вызвать; иначе 404")
    }
}

fn main() {
    let mut routes = Routes::new();
    routes.add("/", |_| Response { status: 200, body: "home".into() });
    println!("{:?}", routes.dispatch("/"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping(_: &Request) -> Response {
        Response { status: 200, body: "pong".to_string() }
    }

    #[test]
    fn functions_and_closures_share_a_table() {
        let mut routes = Routes::new();
        let version = String::from("v1");
        routes.add("/ping", ping);
        routes.add("/version", move |_| Response { status: 200, body: version.clone() });
        routes.add("/echo", |req| Response { status: 200, body: req.path.clone() });
        assert_eq!(routes.dispatch("/ping").body, "pong");
        assert_eq!(routes.dispatch("/version").body, "v1");
        assert_eq!(routes.dispatch("/echo").body, "/echo", "обработчик видит путь запроса");
    }

    #[test]
    fn unknown_path_is_404() {
        let routes = Routes::new();
        assert_eq!(routes.dispatch("/nope"), Response { status: 404, body: "not found".into() });
    }

    #[test]
    fn handler_trait_object_can_be_built_by_hand() {
        let h: Box<dyn Handler> = Box::new(FnHandler(|_: &Request| Response { status: 201, body: "ok".into() }));
        assert_eq!(h.call(&Request { path: "/".into() }).status, 201);
    }

    #[test]
    fn stateful_closures_work() {
        use std::cell::Cell;
        use std::rc::Rc;
        let hits = Rc::new(Cell::new(0));
        let hits2 = Rc::clone(&hits);
        let mut routes = Routes::new();
        routes.add("/count", move |_| {
            hits2.set(hits2.get() + 1);
            Response { status: 200, body: hits2.get().to_string() }
        });
        assert_eq!(routes.dispatch("/count").body, "1");
        assert_eq!(routes.dispatch("/count").body, "2");
        assert_eq!(hits.get(), 2);
    }
}
