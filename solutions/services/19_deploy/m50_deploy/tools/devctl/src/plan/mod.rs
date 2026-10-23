use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use service_runtime::Source;

pub const POSTS_REPLICAS_ENV: &str = "DEVCTL_POSTS_REPLICAS";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub name: String,
    pub service: String,
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub ready: Option<String>,
    pub after: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub units: Vec<Unit>,
}

impl Unit {
    pub fn new(name: &str, service: &str, program: &str) -> Unit {
        Unit {
            name: name.to_string(),
            service: service.to_string(),
            program: program.to_string(),
            args: Vec::new(),
            env: Vec::new(),
            ready: None,
            after: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: &str) -> Unit {
        self.args.push(arg.to_string());
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Unit {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    pub fn ready(mut self, url: &str) -> Unit {
        self.ready = Some(url.to_string());
        self
    }

    pub fn after(mut self, service: &str) -> Unit {
        self.after.push(service.to_string());
        self
    }
}

impl Plan {
    pub fn with(mut self, unit: Unit) -> Plan {
        self.units.push(unit);
        self
    }

    pub fn order(&self) -> Result<Vec<&Unit>, String> {
        let services: BTreeSet<&str> = self
            .units
            .iter()
            .map(|unit| unit.service.as_str())
            .collect();
        for unit in &self.units {
            if let Some(missing) = unit
                .after
                .iter()
                .find(|need| !services.contains(need.as_str()))
            {
                return Err(format!(
                    "{} ждёт {missing}, а такого сервиса в плане нет",
                    unit.name
                ));
            }
        }
        let mut waiting: HashMap<&str, usize> = HashMap::new();
        for unit in &self.units {
            *waiting.entry(unit.service.as_str()).or_default() += 1;
        }
        let mut done: BTreeSet<&str> = BTreeSet::new();
        let mut order = Vec::new();
        let mut left: Vec<&Unit> = self.units.iter().collect();
        while !left.is_empty() {
            let ready: Vec<&Unit> = left
                .iter()
                .copied()
                .filter(|unit| unit.after.iter().all(|need| done.contains(need.as_str())))
                .collect();
            if ready.is_empty() {
                let stuck: Vec<&str> = left.iter().map(|unit| unit.name.as_str()).collect();
                return Err(format!("зависимости по кругу: {}", stuck.join(", ")));
            }
            for unit in &ready {
                order.push(*unit);
                if let Some(count) = waiting.get_mut(unit.service.as_str()) {
                    *count -= 1;
                    if *count == 0 {
                        done.insert(unit.service.as_str());
                    }
                }
            }
            left.retain(|unit| !ready.iter().any(|own| own.name == unit.name));
        }
        Ok(order)
    }
}

fn admin(source: &Source, key: &str, default: &str) -> String {
    format!("http://{}/ready", source.get(key).unwrap_or(default))
}

fn shifted(addr: &str, by: u16) -> String {
    match addr.rsplit_once(':') {
        Some((host, port)) => match port.parse::<u16>() {
            Ok(port) => format!("{host}:{}", port.saturating_add(by)),
            Err(..) => addr.to_string(),
        },
        None => addr.to_string(),
    }
}

pub fn social(source: &Source, bin: &Path) -> Plan {
    let program = |name: &str| bin.join(name).display().to_string();
    let replicas: u16 = source
        .get(POSTS_REPLICAS_ENV)
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(2)
        .clamp(1, 9);
    let posts_grpc = source
        .get("POSTS_GRPC_ADDR")
        .unwrap_or("127.0.0.1:50053")
        .to_string();
    let posts_admin = source
        .get("POSTS_ADMIN_ADDR")
        .unwrap_or("127.0.0.1:9883")
        .to_string();
    let mut plan = Plan::default().with(
        Unit::new("user-service", "user-service", &program("user-service")).ready(&admin(
            source,
            "USERS_ADMIN_ADDR",
            "127.0.0.1:9882",
        )),
    );
    let mut list = Vec::new();
    for replica in 0..replicas {
        let grpc = shifted(&posts_grpc, replica * 10);
        let ops = shifted(&posts_admin, replica * 10);
        list.push(grpc.clone());
        plan = plan.with(
            Unit::new(
                &format!("post-service-{}", replica + 1),
                "post-service",
                &program("post-service"),
            )
            .env("POSTS_GRPC_ADDR", &grpc)
            .env("POSTS_ADMIN_ADDR", &ops)
            .env("POSTS_NODE", &(replica + 1).to_string())
            .ready(&format!("http://{ops}/ready"))
            .after("user-service"),
        );
    }
    plan.with(
        Unit::new("auth-service", "auth-service", &program("auth-service"))
            .ready(&admin(source, "AUTH_ADMIN_ADDR", "127.0.0.1:9881"))
            .after("user-service")
            .after("post-service"),
    )
    .with(
        Unit::new("chat-service", "chat-service", &program("chat-service"))
            .ready(&admin(source, "CHAT_ADMIN_ADDR", "127.0.0.1:9884"))
            .after("user-service"),
    )
    .with(
        Unit::new("feed-service", "feed-service", &program("feed-service")).ready(&admin(
            source,
            "FEED_ADMIN_ADDR",
            "127.0.0.1:9886",
        )),
    )
    .with(
        Unit::new(
            "notify-service",
            "notify-service",
            &program("notify-service"),
        )
        .ready(&admin(source, "NOTIFY_ADMIN_ADDR", "127.0.0.1:9885"))
        .after("user-service"),
    )
    .with(gateway(
        source,
        &program("api-gateway"),
        "api-gateway-1",
        &list,
        0,
    ))
    .with(gateway(
        source,
        &program("api-gateway"),
        "api-gateway-2",
        &list,
        10,
    ))
}

fn gateway(source: &Source, program: &str, name: &str, posts: &[String], by: u16) -> Unit {
    let http = shifted(
        source.get("GATEWAY_HTTP_ADDR").unwrap_or("127.0.0.1:7880"),
        by,
    );
    let ops = shifted(
        source.get("GATEWAY_ADMIN_ADDR").unwrap_or("127.0.0.1:9880"),
        by,
    );
    Unit::new(name, "api-gateway", program)
        .env("GATEWAY_HTTP_ADDR", &http)
        .env("GATEWAY_ADMIN_ADDR", &ops)
        .env("GATEWAY_POSTS_REPLICAS", &posts.join(","))
        .ready(&format!("http://{ops}/ready"))
        .after("auth-service")
        .after("post-service")
        .after("chat-service")
        .after("feed-service")
        .after("notify-service")
}
