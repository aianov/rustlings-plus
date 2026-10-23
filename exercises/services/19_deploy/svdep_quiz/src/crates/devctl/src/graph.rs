use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct ServiceSpec {
    pub name: &'static str,
    pub deps: Vec<&'static str>,
    pub replicas: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StackError {
    Cycle(Vec<String>),
    Unknown(String),
    NotReady(String),
}

/// Имена экземпляров: `name` для одного, `name-1`, `name-2`... для нескольких.
pub fn instances_of(spec: &ServiceSpec) -> Vec<String> {
    if spec.replicas == 1 { vec![spec.name.to_string()] } else { (1..=spec.replicas).map(|i| format!("{}-{i}", spec.name)).collect() }
}

/// Проверка графа: неизвестные зависимости и циклы.
pub fn validate(specs: &[ServiceSpec]) -> Result<(), StackError> {
    let names: HashSet<&str> = specs.iter().map(|s| s.name).collect();
    for s in specs {
        if let Some(missing) = s.deps.iter().find(|d| !names.contains(*d)) {
            return Err(StackError::Unknown((*missing).to_string()));
        }
    }
    fn visit<'a>(name: &'a str, by_name: &HashMap<&'a str, &'a ServiceSpec>, path: &mut Vec<&'a str>, done: &mut HashSet<&'a str>) -> Option<Vec<String>> {
        if let Some(at) = path.iter().position(|p| *p == name) {
            return Some(path[at..].iter().map(|s| s.to_string()).collect());
        }
        if done.contains(name) {
            return None;
        }
        path.push(name);
        for dep in &by_name[name].deps {
            if let Some(found) = visit(dep, by_name, path, done) {
                return Some(found);
            }
        }
        path.pop();
        done.insert(name);
        None
    }
    let by_name: HashMap<&str, &ServiceSpec> = specs.iter().map(|s| (s.name, s)).collect();
    let mut done = HashSet::new();
    match specs.iter().find_map(|s| visit(s.name, &by_name, &mut Vec::new(), &mut done)) {
        Some(cycle) => Err(StackError::Cycle(cycle)),
        None => Ok(()),
    }
}
