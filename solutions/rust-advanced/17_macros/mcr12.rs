#![allow(unused_macros, clippy::vec_init_then_push)]

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

pub struct Request {
    pub method: Method,
    pub path: String,
}

pub type Handler = fn(&Request) -> String;

pub struct Route {
    pub method: Method,
    pub pattern: &'static str,
    pub handler: Handler,
}

impl Route {
    pub fn matches(&self, request: &Request) -> bool {
        if self.method != request.method {
            return false;
        }
        let wanted: Vec<&str> = self.pattern.trim_matches('/').split('/').collect();
        let actual: Vec<&str> = request.path.trim_matches('/').split('/').collect();
        wanted.len() == actual.len() && wanted.iter().zip(&actual).all(|(w, a)| w.starts_with(':') || w == a)
    }
}

pub fn dispatch(routes: &[Route], request: &Request) -> String {
    match routes.iter().find(|route| route.matches(request)) {
        Some(route) => (route.handler)(request),
        None => "404".to_string(),
    }
}

fn list_users(_: &Request) -> String {
    "all users".to_string()
}

fn create_user(_: &Request) -> String {
    "created".to_string()
}

fn show_user(request: &Request) -> String {
    format!("user {}", request.path.rsplit('/').next().unwrap_or(""))
}

fn delete_user(request: &Request) -> String {
    format!("deleted {}", request.path.rsplit('/').next().unwrap_or(""))
}

macro_rules! routes {
    (@acc $table:ident;) => {};
    (@acc $table:ident; GET $pattern:literal => $handler:path; $($rest:tt)*) => {
        $table.push(Route { method: Method::Get, pattern: $pattern, handler: $handler });
        routes!(@acc $table; $($rest)*);
    };
    (@acc $table:ident; POST $pattern:literal => $handler:path; $($rest:tt)*) => {
        $table.push(Route { method: Method::Post, pattern: $pattern, handler: $handler });
        routes!(@acc $table; $($rest)*);
    };
    (@acc $table:ident; PUT $pattern:literal => $handler:path; $($rest:tt)*) => {
        $table.push(Route { method: Method::Put, pattern: $pattern, handler: $handler });
        routes!(@acc $table; $($rest)*);
    };
    (@acc $table:ident; DELETE $pattern:literal => $handler:path; $($rest:tt)*) => {
        $table.push(Route { method: Method::Delete, pattern: $pattern, handler: $handler });
        routes!(@acc $table; $($rest)*);
    };
    (@acc $table:ident; $other:ident $($rest:tt)*) => {
        compile_error!(concat!("неизвестный метод: ", stringify!($other)));
    };
    ($($entries:tt)*) => {{
        #[allow(unused_mut)]
        let mut table: Vec<Route> = Vec::new();
        routes!(@acc table; $($entries)*);
        table
    }};
}

fn main() {
    let table = routes! { GET "/users" => list_users; };
    println!("{}", table.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("mcr12.rs");

    fn compiles_with_lints(extra: &str, deny: &[&str]) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let mut cmd = std::process::Command::new("rustc");
        cmd.args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings"]);
        for lint in deny {
            cmd.args(["-D", lint]);
        }
        let ok = cmd.arg("--out-dir").arg(&dir).arg(&file).output().expect("не удалось запустить rustc").status.success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }

    fn get(path: &str) -> Request {
        Request { method: Method::Get, path: path.to_string() }
    }

    fn table() -> Vec<Route> {
        routes! {
            GET "/users" => list_users;
            POST "/users" => create_user;
            GET "/users/:id" => show_user;
            DELETE "/users/:id" => delete_user;
        }
    }

    #[test]
    fn the_table_keeps_the_written_order() {
        let table = table();
        assert_eq!(table.len(), 4);
        assert_eq!(table[0].pattern, "/users");
        assert_eq!(table[0].method, Method::Get);
        assert_eq!(table[1].method, Method::Post);
        assert_eq!(table[3].pattern, "/users/:id");
        assert_eq!(table[3].method, Method::Delete);
    }

    #[test]
    fn requests_reach_the_right_handlers() {
        let table = table();
        assert_eq!(dispatch(&table, &get("/users")), "all users");
        assert_eq!(dispatch(&table, &get("/users/42")), "user 42");
        assert_eq!(dispatch(&table, &Request { method: Method::Post, path: "/users".into() }), "created");
        assert_eq!(dispatch(&table, &Request { method: Method::Delete, path: "/users/7".into() }), "deleted 7");
        assert_eq!(dispatch(&table, &get("/nowhere")), "404");
    }

    #[test]
    fn an_empty_table_and_put_work() {
        assert!(routes! {}.is_empty());
        let one = routes! { PUT "/users/:id" => create_user; };
        assert_eq!(one[0].method, Method::Put);
    }

    #[test]
    fn handlers_may_be_paths() {
        let table = routes! { GET "/a" => self::list_users; };
        assert_eq!(dispatch(&table, &get("/a")), "all users");
    }

    #[test]
    fn an_unknown_method_stops_the_build() {
        assert!(compiles_with_lints("fn t() -> Vec<Route> { routes! { GET \"/a\" => list_users; } }", &[]), "корректная таблица должна собираться");
        assert!(!compiles_with_lints("fn t() -> Vec<Route> { routes! { PATCH \"/a\" => list_users; } }", &[]), "PATCH не поддерживается и должен давать ошибку сборки");
        assert!(!compiles_with_lints("fn t() -> Vec<Route> { routes! { GET \"/a\" => list_users; FETCH \"/b\" => list_users; } }", &[]), "ошибка и в середине таблицы");
    }
}
