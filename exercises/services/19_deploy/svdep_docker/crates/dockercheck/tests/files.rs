use std::collections::BTreeMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(root().join(name)).unwrap_or_else(|_| panic!("нет файла {name} в корне проекта"))
}

/// Инструкции Dockerfile: продолжения строк (`\`) склеены, комментарии и пустые строки убраны.
fn instructions(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut joined = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if joined.is_empty() && (line.is_empty() || line.starts_with('#')) {
            continue;
        }
        if let Some(stripped) = line.strip_suffix('\\') {
            joined.push_str(stripped);
            joined.push(' ');
            continue;
        }
        joined.push_str(line);
        let full = std::mem::take(&mut joined);
        let (word, rest) = full.split_once(char::is_whitespace).unwrap_or((full.as_str(), ""));
        out.push((word.to_ascii_uppercase(), rest.trim().to_string()));
    }
    out
}

fn stages(ins: &[(String, String)]) -> Vec<Vec<(String, String)>> {
    let mut result: Vec<Vec<(String, String)>> = Vec::new();
    for item in ins {
        if item.0 == "FROM" {
            result.push(Vec::new());
        }
        if let Some(last) = result.last_mut() {
            last.push(item.clone());
        }
    }
    result
}

fn dockerfile() -> Vec<Vec<(String, String)>> {
    stages(&instructions(&read("Dockerfile")))
}

fn is_json_array(s: &str) -> bool {
    s.starts_with('[') && s.ends_with(']') && s.contains('"')
}

#[test]
fn the_image_is_built_in_stages_and_only_the_binaries_ship() {
    let st = dockerfile();
    assert!(st.len() >= 2, "образ собирается в один этап: компилятор Rust и исходники попадут в рабочий образ (гигабайты)");
    let builder_from = &st[0][0].1;
    let last_from = &st.last().unwrap()[0].1;
    assert!(builder_from.to_lowercase().contains(" as "), "этап сборки должен иметь имя (`FROM ... AS builder`): {builder_from}");
    assert!(!last_from.starts_with("rust:") && !last_from.contains("builder"), "рабочий образ не должен быть образом с компилятором: {last_from}");
    let last = st.last().unwrap();
    let copies: Vec<&String> = last.iter().filter(|(w, r)| w == "COPY" && r.contains("--from=")).map(|(_, r)| r).collect();
    assert!(copies.len() >= 2, "из сборочного образа должны копироваться бинарники сервиса и healthcheck: {copies:?}");
    for c in copies {
        let src = c.split_whitespace().find(|p| p.starts_with('/')).unwrap_or("");
        assert!(src.contains("/target/release/"), "копировать надо конкретный готовый бинарник из target/release, а не каталог: {c}");
    }
    assert!(!last.iter().any(|(w, r)| w == "RUN" && (r.contains("cargo") || r.contains("gcc") || r.contains("build-essential"))), "в рабочем образе не должно быть сборки и компилятора");
}

#[test]
fn dependencies_are_cached_in_their_own_layer() {
    let st = dockerfile();
    let builder = &st[0];
    let pos = |pred: &dyn Fn(&(String, String)) -> bool| builder.iter().position(pred);
    let lock = pos(&|(w, r)| w == "COPY" && r.contains("Cargo.lock")).expect("манифесты (Cargo.toml, Cargo.lock) должны копироваться отдельным слоем");
    let first_build = pos(&|(w, r)| w == "RUN" && r.contains("cargo build")).expect("нет сборки");
    let sources = pos(&|(w, r)| w == "COPY" && !r.contains("Cargo.lock") && !r.contains("Cargo.toml") && !r.contains("--from") && (r.starts_with(". ") || r.contains("ms") || r.contains("src") || r.contains("tools"))).expect("не копируются исходники");
    assert!(lock < first_build && first_build < sources, "порядок слоёв: сначала манифесты, потом сборка зависимостей (по заглушке), и только потом исходники: иначе каждая правка кода пересобирает все зависимости");
    let final_build = builder.iter().rposition(|(w, r)| w == "RUN" && r.contains("cargo build")).unwrap();
    assert!(final_build > sources, "после копирования исходников нужна ещё одна сборка");
}

#[test]
fn release_builds_only() {
    let text = read("Dockerfile");
    for (w, r) in instructions(&text) {
        if w == "RUN" && r.contains("cargo build") {
            assert!(r.contains("--release"), "сборка без --release даёт медленный бинарник в сотни мегабайт: {r}");
        }
        assert!(!(w == "CMD" && r.contains("target/debug")), "в образе запускается отладочная сборка");
    }
}

#[test]
fn the_service_does_not_run_as_root() {
    let st = dockerfile();
    let last = st.last().unwrap();
    let user = last.iter().rev().find(|(w, _)| w == "USER").map(|(_, r)| r.to_lowercase()).expect("нет USER: сервис работает от root, и любая дыра в нём это root в контейнере");
    assert!(user != "root" && user != "0" && !user.starts_with("0:") && !user.starts_with("root:"), "USER {user} это root");
    let user_at = last.iter().position(|(w, _)| w == "USER").unwrap();
    let cmd_at = last.iter().position(|(w, _)| w == "CMD" || w == "ENTRYPOINT").expect("нет CMD");
    assert!(user_at < cmd_at, "USER должен стоять до CMD");
}

#[test]
fn the_process_gets_signals_directly() {
    let st = dockerfile();
    let last = st.last().unwrap();
    let (w, r) = last.iter().rev().find(|(w, _)| w == "CMD" || w == "ENTRYPOINT").expect("нет CMD");
    assert!(is_json_array(r), "{w} в shell-форме (`{r}`) запускает сервис через `/bin/sh -c`: SIGTERM получит оболочка, а не сервис, и через 10 секунд Docker убьёт его SIGKILL. Нужна exec-форма: список в квадратных скобках");
    assert!(!r.contains("sh") || !r.contains("-c"), "оболочка в CMD: {r}");
}

#[test]
fn the_healthcheck_needs_no_curl() {
    let st = dockerfile();
    let last = st.last().unwrap();
    let (_, r) = last.iter().find(|(w, _)| w == "HEALTHCHECK").expect("в образе нет HEALTHCHECK");
    assert!(r.contains("healthcheck") && !r.contains("curl") && !r.contains("wget"), "проверка должна вызывать собственный бинарь healthcheck, без curl и wget: {r}");
    assert!(r.contains("--interval") && r.contains("--timeout"), "укажи интервал и таймаут проверки: {r}");
    assert!(last.iter().any(|(w, r)| w == "EXPOSE" && r.chars().any(|c| c.is_ascii_digit())), "нет EXPOSE с портом");
}

#[test]
fn secrets_stay_out_of_the_image() {
    let text = read("Dockerfile");
    for (w, r) in instructions(&text) {
        let upper = r.to_uppercase();
        assert!(!(w == "COPY" && (r.contains(".env") || r.contains("secrets"))), "в образ копируется файл с секретами: {r}");
        if w == "ENV" || w == "ARG" {
            assert!(!["PASSWORD", "SECRET", "TOKEN", "KEY"].iter().any(|k| upper.contains(k)), "секрет запекается в слой образа и виден каждому, у кого есть образ: {w} {r}");
        }
    }
    let ignore = read(".dockerignore");
    let lines: Vec<&str> = ignore.lines().map(str::trim).collect();
    for needed in ["target", ".git", ".env"] {
        assert!(lines.iter().any(|l| l.trim_end_matches('/') == needed), ".dockerignore должен исключать {needed}: иначе контекст сборки огромен и содержит секреты");
    }
}

/// Очень маленький разбор YAML-подмножества: словари по отступам, списки `- x`, скаляры и однострочные списки `[a, b]`.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Map(BTreeMap<String, Node>),
    List(Vec<String>),
    Scalar(String),
}

fn parse_yaml(text: &str) -> Node {
    fn block(lines: &[(usize, String)], i: &mut usize, indent: usize) -> Node {
        let mut map = BTreeMap::new();
        let mut list = Vec::new();
        while *i < lines.len() {
            let (ind, line) = &lines[*i];
            if *ind < indent {
                break;
            }
            if let Some(item) = line.strip_prefix("- ") {
                list.push(item.trim().trim_matches('"').to_string());
                *i += 1;
                continue;
            }
            let (key, value) = line.split_once(':').expect("строка вида `ключ: значение`");
            let (key, value) = (key.trim().trim_matches('"').to_string(), value.trim());
            *i += 1;
            if value.is_empty() {
                let child_indent = lines.get(*i).map_or(0, |(n, _)| *n);
                if child_indent > *ind {
                    let child = block(lines, i, child_indent);
                    map.insert(key, child);
                } else {
                    map.insert(key, Node::Map(BTreeMap::new()));
                }
            } else if value.starts_with('[') {
                let inner = value.trim_start_matches('[').trim_end_matches(']');
                map.insert(key, Node::List(inner.split(',').map(|s| s.trim().trim_matches('"').to_string()).collect()));
            } else {
                map.insert(key, Node::Scalar(value.trim_matches('"').to_string()));
            }
        }
        if list.is_empty() { Node::Map(map) } else { Node::List(list) }
    }
    let lines: Vec<(usize, String)> = text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .map(|l| (l.len() - l.trim_start().len(), l.trim().to_string()))
        .collect();
    let mut i = 0;
    block(&lines, &mut i, 0)
}

fn get<'a>(node: &'a Node, path: &[&str]) -> Option<&'a Node> {
    path.iter().try_fold(node, |n, key| match n {
        Node::Map(m) => m.get(*key),
        _ => None,
    })
}

fn services(compose: &Node) -> Vec<(&String, &Node)> {
    match get(compose, &["services"]) {
        Some(Node::Map(m)) => m.iter().collect(),
        _ => panic!("в compose нет раздела services"),
    }
}

#[test]
fn compose_waits_for_health_not_for_start() {
    let compose = parse_yaml(&read("docker-compose.yml"));
    let svcs = services(&compose);
    assert!(svcs.iter().any(|(n, _)| *n == "nats") && svcs.iter().any(|(n, _)| *n == "post-service") && svcs.iter().any(|(n, _)| *n == "api-gateway"));
    for (name, svc) in &svcs {
        assert!(get(svc, &["healthcheck", "test"]).is_some(), "у сервиса {name} нет healthcheck: зависимым нечего ждать");
    }
    for (name, dep) in [("post-service", "nats"), ("api-gateway", "post-service")] {
        let svc = svcs.iter().find(|(n, _)| *n == name).unwrap().1;
        let cond = get(svc, &["depends_on", dep, "condition"]);
        assert_eq!(cond, Some(&Node::Scalar("service_healthy".to_string())), "{name} должен ждать готовности {dep} (`condition: service_healthy`), а не просто запуска контейнера");
    }
}

#[test]
fn compose_keeps_secrets_in_files_and_ports_closed() {
    let compose = parse_yaml(&read("docker-compose.yml"));
    let text = read("docker-compose.yml");
    assert!(!text.contains("hunter2") && !text.contains("DATABASE_PASSWORD:"), "пароль лежит в compose-файле открытым текстом");
    let post = services(&compose).into_iter().find(|(n, _)| *n == "post-service").unwrap().1;
    let env = get(post, &["environment", "DATABASE_PASSWORD_FILE"]);
    assert_eq!(env, Some(&Node::Scalar("/run/secrets/db_password".to_string())), "пароль передаётся путём к файлу секрета (`DATABASE_PASSWORD_FILE`)");
    assert_eq!(get(post, &["secrets"]), Some(&Node::List(vec!["db_password".to_string()])), "сервису не подключён секрет");
    assert!(get(&compose, &["secrets", "db_password", "file"]).is_some(), "секрет db_password не объявлен в разделе secrets");
    for (name, svc) in services(&compose) {
        let published = get(svc, &["ports"]).is_some();
        assert_eq!(published, name == "api-gateway", "наружу публикуется только api-gateway, а у {name} порты {}", if published { "открыты" } else { "закрыты" });
        assert_eq!(get(svc, &["restart"]), Some(&Node::Scalar("unless-stopped".to_string())), "у {name} нет политики перезапуска");
    }
}
