use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Mutex, OnceLock, PoisonError};

pub const BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

type Labels = Vec<(String, String)>;

#[derive(Debug, Clone, PartialEq)]
enum Series {
    Counter(u64),
    Gauge(i64),
    Histogram {
        counts: Vec<u64>,
        sum: f64,
        count: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Counter,
    Gauge,
    Histogram,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Counter => "counter",
            Kind::Gauge => "gauge",
            Kind::Histogram => "histogram",
        }
    }
}

#[derive(Debug)]
struct Family {
    help: String,
    kind: Kind,
    series: BTreeMap<Labels, Series>,
}

#[derive(Debug, Default)]
pub struct Metrics {
    families: Mutex<BTreeMap<String, Family>>,
}

pub fn metrics() -> &'static Metrics {
    static GLOBAL: OnceLock<Metrics> = OnceLock::new();
    GLOBAL.get_or_init(Metrics::default)
}

fn owned(labels: &[(&str, &str)]) -> Labels {
    let mut labels: Labels = labels
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect();
    labels.sort();
    labels
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn label_text(labels: &Labels, extra: Option<(&str, String)>) -> String {
    let mut parts: Vec<String> = labels
        .iter()
        .map(|(name, value)| format!("{name}=\"{}\"", escape(value)))
        .collect();
    if let Some((name, value)) = extra {
        parts.push(format!("{name}=\"{value}\""));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", parts.join(","))
    }
}

fn number(value: f64) -> String {
    if value.is_infinite() {
        String::from("+Inf")
    } else {
        format!("{value}")
    }
}

impl Metrics {
    pub fn new() -> Metrics {
        Metrics::default()
    }

    fn with(
        &self,
        name: &str,
        help: &str,
        kind: Kind,
        labels: &[(&str, &str)],
        change: impl FnOnce(&mut Series),
    ) {
        let mut families = self.families.lock().unwrap_or_else(PoisonError::into_inner);
        let family = families.entry(name.to_string()).or_insert_with(|| Family {
            help: help.to_string(),
            kind,
            series: BTreeMap::new(),
        });
        if family.kind != kind {
            return;
        }
        let series = family
            .series
            .entry(owned(labels))
            .or_insert_with(|| match kind {
                Kind::Counter => Series::Counter(0),
                Kind::Gauge => Series::Gauge(0),
                Kind::Histogram => Series::Histogram {
                    counts: vec![0; BUCKETS.len()],
                    sum: 0.0,
                    count: 0,
                },
            });
        change(series);
    }

    pub fn inc(&self, name: &str, help: &str, labels: &[(&str, &str)], by: u64) {
        self.with(name, help, Kind::Counter, labels, |series| {
            if let Series::Counter(total) = series {
                *total = total.saturating_add(by);
            }
        });
    }

    pub fn set(&self, name: &str, help: &str, labels: &[(&str, &str)], value: i64) {
        self.with(name, help, Kind::Gauge, labels, |series| {
            if let Series::Gauge(current) = series {
                *current = value;
            }
        });
    }

    pub fn add(&self, name: &str, help: &str, labels: &[(&str, &str)], delta: i64) {
        self.with(name, help, Kind::Gauge, labels, |series| {
            if let Series::Gauge(current) = series {
                *current = current.saturating_add(delta);
            }
        });
    }

    pub fn observe(&self, name: &str, help: &str, labels: &[(&str, &str)], value: f64) {
        self.with(name, help, Kind::Histogram, labels, |series| {
            if let Series::Histogram { counts, sum, count } = series {
                for (bound, slot) in BUCKETS.iter().zip(counts.iter_mut()) {
                    if value <= *bound {
                        *slot += 1;
                    }
                }
                *sum += value;
                *count += 1;
            }
        });
    }

    pub fn render(&self) -> String {
        let families = self.families.lock().unwrap_or_else(PoisonError::into_inner);
        let mut out = String::new();
        for (name, family) in families.iter() {
            let _ = writeln!(out, "# HELP {name} {}", family.help.replace('\n', " "));
            let _ = writeln!(out, "# TYPE {name} {}", family.kind.name());
            for (labels, series) in &family.series {
                match series {
                    Series::Counter(total) => {
                        let _ = writeln!(out, "{name}{} {total}", label_text(labels, None));
                    }
                    Series::Gauge(value) => {
                        let _ = writeln!(out, "{name}{} {value}", label_text(labels, None));
                    }
                    Series::Histogram { counts, sum, count } => {
                        for (bound, seen) in BUCKETS.iter().zip(counts) {
                            let le = Some(("le", number(*bound)));
                            let _ = writeln!(out, "{name}_bucket{} {seen}", label_text(labels, le));
                        }
                        let inf = Some(("le", number(f64::INFINITY)));
                        let _ = writeln!(out, "{name}_bucket{} {count}", label_text(labels, inf));
                        let _ = writeln!(out, "{name}_sum{} {sum}", label_text(labels, None));
                        let _ = writeln!(out, "{name}_count{} {count}", label_text(labels, None));
                    }
                }
            }
        }
        out
    }
}
