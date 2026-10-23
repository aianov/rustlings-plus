use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Version {
        Version { major, minor, patch }
    }

    pub fn parse(s: &str) -> Option<Version> {
        let mut parts = s.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Version { major, minor, patch })
    }
}

#[derive(Clone, Copy)]
enum Op {
    Caret,
    Tilde,
    Exact,
}

struct Parts {
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
}

fn parse_parts(s: &str) -> Option<Parts> {
    let mut it = s.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = match it.next() {
        Some(m) => Some(m.parse().ok()?),
        None => None,
    };
    let patch = match it.next() {
        Some(p) => Some(p.parse().ok()?),
        None => None,
    };
    if it.next().is_some() {
        return None;
    }
    Some(Parts { major, minor, patch })
}

#[derive(Debug)]
pub struct Req {
    lower: Version,
    upper: Option<Version>,
}

impl Req {
    pub fn parse(s: &str) -> Option<Req> {
        if s == "*" {
            return Some(Req { lower: Version::new(0, 0, 0), upper: None });
        }
        let (op, rest) = match s.chars().next()? {
            '^' => (Op::Caret, &s[1..]),
            '~' => (Op::Tilde, &s[1..]),
            '=' => (Op::Exact, &s[1..]),
            _ => match s.strip_suffix(".*") {
                Some(prefix) => (Op::Tilde, prefix),
                None => (Op::Caret, s),
            },
        };
        let parts = parse_parts(rest)?;
        if matches!(op, Op::Exact) && parts.patch.is_none() {
            return None;
        }
        let lower = Version::new(parts.major, parts.minor.unwrap_or(0), parts.patch.unwrap_or(0));
        Some(Req { lower, upper: Some(upper_bound(op, &parts)) })
    }

    pub fn matches(&self, v: &Version) -> bool {
        *v >= self.lower && self.upper.is_none_or(|u| *v < u)
    }
}

pub fn matches(req: &str, version: &str) -> bool {
    match (Req::parse(req), Version::parse(version)) {
        (Some(req), Some(version)) => req.matches(&version),
        _ => false,
    }
}
fn upper_bound(op: Op, p: &Parts) -> Version {
    match op {
        Op::Exact => Version::new(p.major, p.minor.unwrap_or(0), p.patch.unwrap_or(0) + 1),
        Op::Tilde => match p.minor {
            Some(minor) => Version::new(p.major, minor + 1, 0),
            None => Version::new(p.major + 1, 0, 0),
        },
        Op::Caret => match (p.major, p.minor, p.patch) {
            (major, _, _) if major > 0 => Version::new(major + 1, 0, 0),
            (_, None, _) => Version::new(1, 0, 0),
            (_, Some(minor), _) if minor > 0 => Version::new(0, minor + 1, 0),
            (_, Some(_), None) => Version::new(0, 1, 0),
            (_, Some(_), Some(patch)) => Version::new(0, 0, patch + 1),
        },
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

pub fn resolve(req: &str, published: &[&str]) -> Option<String> {
    let req = Req::parse(req)?;
    published
        .iter()
        .filter_map(|s| Version::parse(s))
        .filter(|v| req.matches(v))
        .max()
        .map(|v| v.to_string())
}

#[derive(Debug, PartialEq, Eq)]
pub enum LockError {
    Missing,
    Stale { locked: String },
}

#[derive(Debug, Default)]
pub struct Lock {
    entries: BTreeMap<String, String>,
}

impl Lock {
    pub fn new() -> Lock {
        Lock::default()
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(String::as_str)
    }

    pub fn build(&mut self, name: &str, req: &str, published: &[&str]) -> Option<String> {
        if let Some(locked) = self.get(name)
            && matches(req, locked)
        {
            return Some(locked.to_string());
        }
        self.update(name, req, published)
    }

    pub fn update(&mut self, name: &str, req: &str, published: &[&str]) -> Option<String> {
        let version = resolve(req, published)?;
        self.entries.insert(name.to_string(), version.clone());
        Some(version)
    }

    pub fn build_locked(&self, name: &str, req: &str) -> Result<String, LockError> {
        match self.get(name) {
            None => Err(LockError::Missing),
            Some(locked) if matches(req, locked) => Ok(locked.to_string()),
            Some(locked) => Err(LockError::Stale { locked: locked.to_string() }),
        }
    }
}

fn main() {
    let published = ["1.0.100", "1.0.200", "2.0.0"];
    let mut lock = Lock::new();
    println!("{:?}", lock.build("serde", "^1.0.100", &published));
    println!("{:?}", lock.build_locked("serde", "^1.0.100"));
    println!("{:?}", lock.update("serde", "^1.0.100", &published));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERDE: [&str; 5] = ["1.0.150", "2.0.0", "1.0.100", "1.0.200", "0.9.0"];

    #[test]
    fn resolve_picks_the_greatest_match() {
        assert_eq!(resolve("^1.0.100", &SERDE).as_deref(), Some("1.0.200"), "из 1.0.x и 1.x надо взять максимальную, а не последнюю в списке");
        assert_eq!(resolve("^2", &SERDE).as_deref(), Some("2.0.0"), "^2 подходит только 2.0.0");
        assert_eq!(resolve("*", &SERDE).as_deref(), Some("2.0.0"), "звёздочка берёт самую новую");
        assert_eq!(resolve("=1.0.150", &SERDE).as_deref(), Some("1.0.150"), "= берёт ровно одну версию");
    }

    #[test]
    fn resolve_compares_numbers_not_text() {
        let published = ["1.9.0", "1.10.0", "1.2.0"];
        assert_eq!(resolve("^1", &published).as_deref(), Some("1.10.0"), "1.10.0 новее 1.9.0");
    }

    #[test]
    fn resolve_without_match_is_none() {
        assert_eq!(resolve("^3", &SERDE), None, "версии 3.x нет в реестре");
        assert_eq!(resolve("^banana", &SERDE), None, "кривое требование не должно ничего находить");
        assert_eq!(resolve("^1", &[]), None, "в пустом реестре искать нечего");
    }

    #[test]
    fn resolve_skips_garbage_versions() {
        let published = ["1.0.0", "oops", "1.2", "1.1.0"];
        assert_eq!(resolve("^1", &published).as_deref(), Some("1.1.0"), "кривые строки в реестре надо пропускать");
    }

    #[test]
    fn build_writes_the_lock_on_first_run() {
        let mut lock = Lock::new();
        assert_eq!(lock.get("serde"), None, "новый лок пуст");
        assert_eq!(lock.build("serde", "^1.0.100", &SERDE).as_deref(), Some("1.0.200"));
        assert_eq!(lock.get("serde"), Some("1.0.200"), "после первой сборки версия должна оказаться в локе");
    }

    #[test]
    fn build_keeps_locked_version_when_newer_appears() {
        let mut lock = Lock::new();
        lock.build("serde", "^1.0.100", &SERDE);
        let later = ["1.0.100", "1.0.200", "1.0.250", "2.0.0"];
        assert_eq!(
            lock.build("serde", "^1.0.100", &later).as_deref(),
            Some("1.0.200"),
            "лок существует ровно для этого: вышла 1.0.250, а сборка осталась на 1.0.200"
        );
        assert_eq!(lock.get("serde"), Some("1.0.200"), "build не должен трогать подходящую запись");
    }

    #[test]
    fn build_resolves_again_when_requirement_changed() {
        let mut lock = Lock::new();
        lock.build("serde", "^1.0.100", &SERDE);
        assert_eq!(
            lock.build("serde", "^2", &SERDE).as_deref(),
            Some("2.0.0"),
            "требование изменили в Cargo.toml, а записанная версия под него не подходит: нужен новый выбор"
        );
        assert_eq!(lock.get("serde"), Some("2.0.0"), "новый выбор должен попасть в лок");
    }

    #[test]
    fn update_moves_only_the_named_package() {
        let mut lock = Lock::new();
        lock.build("serde", "^1.0.100", &SERDE);
        lock.build("tokio", "^1.0.100", &SERDE);
        let later = ["1.0.100", "1.0.200", "1.0.250"];
        assert_eq!(lock.update("serde", "^1.0.100", &later).as_deref(), Some("1.0.250"), "update берёт максимальную");
        assert_eq!(lock.get("serde"), Some("1.0.250"), "update должен перезаписать запись");
        assert_eq!(lock.get("tokio"), Some("1.0.200"), "cargo update -p serde не должен трогать tokio");
    }

    #[test]
    fn update_with_no_match_keeps_the_old_entry() {
        let mut lock = Lock::new();
        lock.build("serde", "^1.0.100", &SERDE);
        assert_eq!(lock.update("serde", "^7", &SERDE), None, "версии 7.x нет");
        assert_eq!(lock.get("serde"), Some("1.0.200"), "неудачный update не должен стирать рабочую запись");
    }

    #[test]
    fn locked_build_never_writes() {
        let mut lock = Lock::new();
        assert_eq!(lock.build_locked("serde", "^1"), Err(LockError::Missing), "в пустом локе --locked должен отказать");
        assert_eq!(lock.get("serde"), None, "--locked не имеет права записывать в лок");
        lock.build("serde", "^1.0.100", &SERDE);
        assert_eq!(lock.build_locked("serde", "^1.0.100"), Ok("1.0.200".to_string()), "подходящая запись это успех");
        assert_eq!(
            lock.build_locked("serde", "^2"),
            Err(LockError::Stale { locked: "1.0.200".to_string() }),
            "требование ушло вперёд, а лок остался старым: --locked должен остановить сборку"
        );
        assert_eq!(lock.get("serde"), Some("1.0.200"), "--locked не должен менять запись");
    }
}
