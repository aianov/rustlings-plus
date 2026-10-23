use post_service::bootstrap;
use post_service::controller::{Request, Response};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rs_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}

fn layer_sources(layer: &str) -> Vec<(PathBuf, String)> {
    rs_files(&root().join(layer))
        .into_iter()
        .map(|p| {
            let text = fs::read_to_string(&p).unwrap();
            (p, text)
        })
        .collect()
}

fn forbid(layer: &str, needles: &[&str], why: &str) {
    for (path, text) in layer_sources(layer) {
        for needle in needles {
            assert!(
                !text.contains(needle),
                "в {} найдено `{needle}`: {why}",
                path.display()
            );
        }
    }
}

fn request(method: &str, path: &str, body: &str) -> Request {
    Request { method: method.to_string(), path: path.to_string(), body: body.to_string() }
}

fn json(resp: &Response) -> Value {
    serde_json::from_str(&resp.body).expect("тело ответа должно быть JSON")
}

#[test]
fn service_has_the_agreed_layout() {
    let src = root();
    for file in ["main.rs", "lib.rs"] {
        assert!(src.join(file).is_file(), "нет src/{file}");
    }
    for layer in ["bootstrap", "controller", "dto", "service", "repository", "common"] {
        assert!(
            src.join(layer).join("mod.rs").is_file(),
            "нет src/{layer}/mod.rs: каждый слой это папка-модуль"
        );
        assert!(
            !src.join(format!("{layer}.rs")).exists(),
            "src/{layer}.rs лежит рядом с папкой src/{layer}/: оставь что-то одно"
        );
    }
    assert!(src.join("repository/memory.rs").is_file(), "реализация в памяти лежит в src/repository/memory.rs");
    let dto = fs::read_to_string(src.join("dto/mod.rs")).unwrap();
    assert!(dto.contains("CreatePostDto") && dto.contains("PostDto"), "DTO живут в src/dto/mod.rs, а не в controller");
    let common = fs::read_to_string(src.join("common/mod.rs")).unwrap();
    assert!(common.contains("struct Post"), "доменный тип Post живёт в src/common/mod.rs");
    for name in ["utils.rs", "helpers.rs"] {
        let bad: Vec<_> = rs_files(&src).into_iter().filter(|p| p.ends_with(name)).collect();
        assert!(bad.is_empty(), "файл {name} это мусорная корзина без смысла: {bad:?}");
    }
}

#[test]
fn main_is_thin() {
    let main = fs::read_to_string(root().join("main.rs")).unwrap();
    assert!(main.lines().count() <= 20, "main.rs длиннее 20 строк: вся работа должна жить в библиотеке");
    assert!(main.contains("post_service::"), "main.rs должен вызывать библиотеку");
    assert!(!main.contains("Mutex"), "в main.rs нет хранилища: его собирает bootstrap");
}

#[test]
fn arrows_point_down() {
    forbid("controller", &["repository::"], "контроллер не ходит в репозиторий в обход service");
    forbid(
        "service",
        &["serde_json", "controller::", "dto::", "Request", "Response"],
        "service не знает ни о JSON, ни о DTO, ни о запросах",
    );
    forbid(
        "repository",
        &["service::", "controller::", "dto::", "serde"],
        "репозиторий не знает о верхних слоях",
    );
    forbid("common", &["service::", "controller::", "repository::", "dto::"], "common лежит под всеми слоями");
}

#[test]
fn create_and_read_a_post() {
    let app = bootstrap::build();
    let created = app.handle(request("POST", "/posts", r#"{"author":1,"text":"  привет, мир "}"#));
    assert_eq!(created.status, 201, "успешное создание это 201, а пришло: {}", created.body);
    assert_eq!(
        json(&created),
        serde_json::json!({"id": 1, "author": 1, "text": "привет, мир"}),
        "тело ответа: id, author и обрезанный text"
    );
    let read = app.handle(request("GET", "/posts/1", ""));
    assert_eq!(read.status, 200, "созданный пост читается: {}", read.body);
    assert_eq!(json(&read)["text"], "привет, мир", "читается тот же текст");
}

#[test]
fn ids_grow_per_application() {
    let first = bootstrap::build();
    let second = bootstrap::build();
    first.handle(request("POST", "/posts", r#"{"author":1,"text":"a"}"#));
    let resp = second.handle(request("POST", "/posts", r#"{"author":1,"text":"b"}"#));
    assert_eq!(json(&resp)["id"], 1, "у каждой сборки своё хранилище: глобального состояния нет");
}

#[test]
fn bad_input_is_rejected_with_the_right_status() {
    let app = bootstrap::build();
    let cases = [
        ("POST", "/posts", "{oops", 400, "bad_json"),
        ("POST", "/posts", r#"{"author":1,"text":"   "}"#, 422, "empty_text"),
        ("GET", "/posts/abc", "", 400, "bad_id"),
        ("GET", "/posts/99", "", 404, "post_not_found"),
        ("GET", "/nope", "", 404, "not_found"),
    ];
    for (method, path, body, status, code) in cases {
        let resp = app.handle(request(method, path, body));
        assert_eq!(resp.status, status, "{method} {path}: ждали {status}, пришло {}", resp.status);
        assert_eq!(json(&resp)["error"], code, "{method} {path}: код ошибки в теле");
    }
}

#[test]
fn text_limit_is_in_chars() {
    let app = bootstrap::build();
    let ok = format!(r#"{{"author":1,"text":"{}"}}"#, "я".repeat(280));
    assert_eq!(app.handle(request("POST", "/posts", &ok)).status, 201, "280 символов проходят");
    let long = format!(r#"{{"author":1,"text":"{}"}}"#, "я".repeat(281));
    let resp = app.handle(request("POST", "/posts", &long));
    assert_eq!(resp.status, 422, "281 символ это уже много");
    assert_eq!(json(&resp)["error"], "text_too_long", "код ошибки text_too_long");
}
