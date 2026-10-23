// Писать в каждом обработчике `Response { status: 200, body: ... }` утомительно. Хочется возвращать
// что удобно: строку, пару «статус и тело», `Result` с ошибкой. Всё это превращает в ответ трейт
// `IntoResponse`. Обёртка `FnHandler<F>` хранит только функцию, а тип её результата `R` в реализации
// трейта `Handler` появляется из границы `F: Fn(&Request) -> R`, и этого компилятору достаточно.
// Правила: `String` и `&'static str` это статус 200. `(u16, String)` берёт статус из пары. `Result<T, E>`:
// `Ok` превращается в ответ как `T`; `Err` как `E`, и если у ошибки получился статус 200, он заменяется на 500.
// TODO: Реализуй `IntoResponse` для всех четырёх случаев и самого `Response`, затем `Handler` для `FnHandler`.

use std::collections::HashMap;

pub struct Request {
    pub path: String,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

pub trait Handler {
    fn call(&self, request: &Request) -> Response;
}

pub struct FnHandler<F> {
    f: F,
}

pub struct Routes {
    table: HashMap<String, Box<dyn Handler>>,
}

impl Routes {
    pub fn new() -> Routes {
        Routes { table: HashMap::new() }
    }

    pub fn add<F, R>(&mut self, path: &str, handler: F)
    where
        F: Fn(&Request) -> R + 'static,
        R: IntoResponse + 'static,
    {
        todo!("обернуть и положить в таблицу")
    }

    pub fn dispatch(&self, path: &str) -> Response {
        match self.table.get(path) {
            Some(handler) => handler.call(&Request { path: path.to_string() }),
            None => Response { status: 404, body: "not found".to_string() },
        }
    }
}

fn main() {
    let mut routes = Routes::new();
    routes.add("/", |_| "home");
    println!("{:?}", routes.dispatch("/"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_return_types() {
        let mut routes = Routes::new();
        routes.add("/s", |_| "static text");
        routes.add("/o", |r| format!("path {}", r.path));
        routes.add("/t", |_| (201, "created".to_string()));
        assert_eq!(routes.dispatch("/s"), Response { status: 200, body: "static text".into() });
        assert_eq!(routes.dispatch("/o"), Response { status: 200, body: "path /o".into() });
        assert_eq!(routes.dispatch("/t"), Response { status: 201, body: "created".into() });
    }

    #[test]
    fn result_return_types() {
        let mut routes = Routes::new();
        routes.add("/ok", |_| -> Result<String, String> { Ok("fine".into()) });
        routes.add("/err", |_| -> Result<String, String> { Err("broken".into()) });
        routes.add("/err404", |_| -> Result<String, (u16, String)> { Err((404, "gone".into())) });
        assert_eq!(routes.dispatch("/ok"), Response { status: 200, body: "fine".into() });
        assert_eq!(routes.dispatch("/err"), Response { status: 500, body: "broken".into() }, "ошибка без своего статуса получает 500");
        assert_eq!(routes.dispatch("/err404"), Response { status: 404, body: "gone".into() }, "собственный статус ошибки сохраняется");
    }

    #[test]
    fn ready_made_responses_pass_through() {
        let mut routes = Routes::new();
        routes.add("/raw", |_| Response { status: 418, body: "teapot".into() });
        assert_eq!(routes.dispatch("/raw").status, 418);
        assert_eq!(routes.dispatch("/missing").status, 404);
    }

    #[test]
    fn nested_results_work() {
        let r: Result<Result<String, (u16, String)>, String> = Ok(Err((403, "no".into())));
        assert_eq!(r.into_response(), Response { status: 403, body: "no".into() });
    }
}
