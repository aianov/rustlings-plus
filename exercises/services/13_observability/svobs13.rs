// Счётчик запросов `http_requests_total{method, route, status}` стоит на графике дашборда. Через неделю Prometheus съедает всю
// память: каждый уникальный путь `/users/123`, `/users/124`... стал отдельным рядом, а ещё в метрики попадают пути от сканеров
// вроде `/wp-login.php` и странные методы. Цель: число рядов зависит от устройства сервиса (его маршрутов, методов и
// кодов), а не от того, что присылают клиенты. В метке `route` должен быть шаблон маршрута, у чужого пути особое значение
// «unmatched», странные методы собираются в «OTHER». Сам счёт запросов при этом остаётся точным.


use std::collections::BTreeMap;
use std::sync::Mutex;

fn main() {}

#[derive(Default)]
pub struct CounterVec {
    series: Mutex<BTreeMap<Vec<(String, String)>, u64>>,
}

impl CounterVec {
    pub fn inc(&self, labels: &[(&str, &str)]) {
        let key: Vec<(String, String)> = labels.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        *self.series.lock().expect("ряды не отравлены").entry(key).or_insert(0) += 1;
    }

    pub fn get(&self, labels: &[(&str, &str)]) -> u64 {
        let key: Vec<(String, String)> = labels.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        self.series.lock().expect("ряды не отравлены").get(&key).copied().unwrap_or(0)
    }

    pub fn series_count(&self) -> usize {
        self.series.lock().expect("ряды не отравлены").len()
    }
}

pub struct Router {
    patterns: Vec<&'static str>,
}

impl Router {
    pub fn new(patterns: &[&'static str]) -> Router {
        Router { patterns: patterns.to_vec() }
    }

    pub fn matched(&self, target: &str) -> Option<&'static str> {
        let path = target.split(['?', '#']).next().unwrap_or("");
        let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
        let parts: Vec<&str> = path.split('/').collect();
        self.patterns.iter().copied().find(|pattern| {
            let want: Vec<&str> = pattern.split('/').collect();
            want.len() == parts.len() && want.iter().zip(&parts).all(|(w, p)| if w.starts_with(':') { !p.is_empty() } else { w == p })
        })
    }
}

pub fn record(requests: &CounterVec, router: &Router, method: &str, target: &str, status: u16) {
    let _ = router;
    requests.inc(&[("method", method), ("route", target), ("status", &status.to_string())]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> Router {
        Router::new(&["/", "/users/:id", "/users/:id/posts", "/posts/:id/comments/:comment_id", "/health"])
    }

    #[test]
    fn thousands_of_ids_make_one_series_and_the_count_stays_exact() {
        let (requests, router) = (CounterVec::default(), router());
        for id in 0..5000 {
            record(&requests, &router, "GET", &format!("/users/{id}"), 200);
        }
        assert_eq!(requests.series_count(), 1, "каждый id породил отдельный ряд: рядов {}", requests.series_count());
        assert_eq!(requests.get(&[("method", "GET"), ("route", "/users/:id"), ("status", "200")]), 5000, "в метке route шаблон маршрута, а счёт точный");
    }

    #[test]
    fn nested_routes_have_their_own_templates() {
        let (requests, router) = (CounterVec::default(), router());
        record(&requests, &router, "GET", "/users/9/posts", 200);
        record(&requests, &router, "POST", "/posts/77/comments/5", 201);
        record(&requests, &router, "GET", "/", 200);
        assert_eq!(requests.get(&[("method", "GET"), ("route", "/users/:id/posts"), ("status", "200")]), 1);
        assert_eq!(requests.get(&[("method", "POST"), ("route", "/posts/:id/comments/:comment_id"), ("status", "201")]), 1);
        assert_eq!(requests.get(&[("method", "GET"), ("route", "/"), ("status", "200")]), 1);
    }

    #[test]
    fn query_strings_and_trailing_slashes_do_not_make_new_routes() {
        let (requests, router) = (CounterVec::default(), router());
        for target in ["/users/5", "/users/5/", "/users/5?token=abc", "/users/5?page=2&sort=asc"] {
            record(&requests, &router, "GET", target, 200);
        }
        assert_eq!(requests.series_count(), 1, "один маршрут, один ряд");
        assert_eq!(requests.get(&[("method", "GET"), ("route", "/users/:id"), ("status", "200")]), 4);
    }

    #[test]
    fn scanner_traffic_collapses_into_one_unmatched_series() {
        let (requests, router) = (CounterVec::default(), router());
        for i in 0..1000 {
            record(&requests, &router, "GET", &format!("/wp-login-{i}.php"), 404);
            record(&requests, &router, "GET", &format!("/a/{i}/b/{i}/c"), 404);
        }
        assert_eq!(requests.series_count(), 1, "чужие пути не должны размножать ряды: {}", requests.series_count());
        assert_eq!(requests.get(&[("method", "GET"), ("route", "unmatched"), ("status", "404")]), 2000);
    }

    #[test]
    fn made_up_methods_become_other() {
        let (requests, router) = (CounterVec::default(), router());
        for method in ["BREW", "get", "PROPFIND", "X-ATTACK-1", "X-ATTACK-2", ""] {
            record(&requests, &router, method, "/health", 405);
        }
        record(&requests, &router, "GET", "/health", 200);
        assert_eq!(requests.get(&[("method", "OTHER"), ("route", "/health"), ("status", "405")]), 6, "метод пришёл от клиента, как и путь: ряды должны быть ограничены");
        assert_eq!(requests.get(&[("method", "GET"), ("route", "/health"), ("status", "200")]), 1, "настоящие методы остаются как есть");
    }

    #[test]
    fn total_number_of_series_is_bounded_by_the_service_not_the_clients() {
        let (requests, router) = (CounterVec::default(), router());
        for i in 0..3000 {
            let target = match i % 4 {
                0 => format!("/users/{i}"),
                1 => format!("/users/{i}/posts"),
                2 => format!("/random-{i}"),
                _ => format!("/posts/{i}/comments/{}", i * 7),
            };
            record(&requests, &router, if i % 50 == 0 { "TRACE" } else { "GET" }, &target, if i % 9 == 0 { 500 } else { 200 });
        }
        assert!(requests.series_count() <= 24, "рядов {}: их число не должно расти с трафиком", requests.series_count());
    }
}
