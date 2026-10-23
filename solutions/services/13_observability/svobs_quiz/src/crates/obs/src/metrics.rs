use std::collections::BTreeMap;
use std::sync::Mutex;

type Labels = Vec<(String, String)>;

struct Hist {
    counts: Vec<u64>,
    sum: f64,
}

enum Kind {
    Counter(BTreeMap<Labels, u64>),
    Gauge(i64),
    Histogram { bounds: Vec<f64>, series: BTreeMap<Labels, Hist> },
}

struct Family {
    help: String,
    kind: Kind,
}

#[derive(Default)]
pub struct Registry {
    families: Mutex<BTreeMap<String, Family>>,
}

fn labels_of(labels: &[(&str, &str)]) -> Labels {
    let mut out: Labels = labels.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    out.sort();
    out
}

fn escape_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn braces(labels: &Labels, extra: Option<(&str, String)>) -> String {
    let mut parts: Vec<String> = labels.iter().map(|(k, v)| format!("{k}=\"{}\"", escape_value(v))).collect();
    if let Some((k, v)) = extra {
        parts.push(format!("{k}=\"{v}\""));
    }
    if parts.is_empty() { String::new() } else { format!("{{{}}}", parts.join(",")) }
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    pub fn inc(&self, name: &str, help: &str, labels: &[(&str, &str)]) {
        let mut families = self.families.lock().expect("реестр не отравлен");
        let family = families.entry(name.to_string()).or_insert_with(|| Family { help: help.to_string(), kind: Kind::Counter(BTreeMap::new()) });
        match &mut family.kind {
            Kind::Counter(series) => *series.entry(labels_of(labels)).or_insert(0) += 1,
            _ => panic!("метрика {name} уже зарегистрирована с другим видом"),
        }
    }

    pub fn gauge_add(&self, name: &str, help: &str, delta: i64) {
        let mut families = self.families.lock().expect("реестр не отравлен");
        let family = families.entry(name.to_string()).or_insert_with(|| Family { help: help.to_string(), kind: Kind::Gauge(0) });
        match &mut family.kind {
            Kind::Gauge(value) => *value += delta,
            _ => panic!("метрика {name} уже зарегистрирована с другим видом"),
        }
    }

    pub fn observe(&self, name: &str, help: &str, bounds: &[f64], labels: &[(&str, &str)], value: f64) {
        if value.is_nan() {
            return;
        }
        let mut families = self.families.lock().expect("реестр не отравлен");
        let family = families.entry(name.to_string()).or_insert_with(|| Family { help: help.to_string(), kind: Kind::Histogram { bounds: bounds.to_vec(), series: BTreeMap::new() } });
        match &mut family.kind {
            Kind::Histogram { bounds, series } => {
                let hist = series.entry(labels_of(labels)).or_insert_with(|| Hist { counts: vec![0; bounds.len() + 1], sum: 0.0 });
                hist.counts[bounds.partition_point(|b| *b < value)] += 1;
                hist.sum += value;
            }
            _ => panic!("метрика {name} уже зарегистрирована с другим видом"),
        }
    }

    pub fn render(&self) -> String {
        let families = self.families.lock().expect("реестр не отравлен");
        let mut out = String::new();
        for (name, family) in families.iter() {
            let help = family.help.replace('\\', "\\\\").replace('\n', "\\n");
            match &family.kind {
                Kind::Counter(series) => {
                    out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} counter\n"));
                    for (labels, n) in series {
                        out.push_str(&format!("{name}{} {n}\n", braces(labels, None)));
                    }
                }
                Kind::Gauge(value) => {
                    out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}\n"));
                }
                Kind::Histogram { bounds, series } => {
                    out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} histogram\n"));
                    for (labels, hist) in series {
                        let mut running = 0;
                        for (bound, n) in bounds.iter().zip(&hist.counts) {
                            running += n;
                            out.push_str(&format!("{name}_bucket{} {running}\n", braces(labels, Some(("le", bound.to_string())))));
                        }
                        running += hist.counts[bounds.len()];
                        out.push_str(&format!("{name}_bucket{} {running}\n", braces(labels, Some(("le", "+Inf".to_string())))));
                        out.push_str(&format!("{name}_sum{} {}\n{name}_count{} {running}\n", braces(labels, None), hist.sum, braces(labels, None)));
                    }
                }
            }
        }
        out
    }
}
