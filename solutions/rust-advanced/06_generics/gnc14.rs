use std::marker::PhantomData;

pub struct Disconnected;
pub struct Connected;
pub struct Authed;

pub struct Session<S> {
    host: String,
    user: Option<String>,
    sent: Vec<String>,
    _state: PhantomData<S>,
}

impl<S> Session<S> {
    fn into_state<T>(self) -> Session<T> {
        Session { host: self.host, user: self.user, sent: self.sent, _state: PhantomData }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn history(&self) -> &[String] {
        &self.sent
    }
}

impl Session<Disconnected> {
    pub fn new(host: &str) -> Self {
        Session { host: host.to_string(), user: None, sent: Vec::new(), _state: PhantomData }
    }

    pub fn connect(self) -> Session<Connected> {
        self.into_state()
    }
}

impl Session<Connected> {
    pub fn login(self, user: &str, password: &str) -> Result<Session<Authed>, (Session<Connected>, String)> {
        let expected: String = user.chars().rev().collect();
        if password != expected {
            return Err((self, format!("неверный пароль для {user}")));
        }
        let mut next: Session<Authed> = self.into_state();
        next.user = Some(user.to_string());
        Ok(next)
    }

    pub fn disconnect(self) -> Session<Disconnected> {
        self.into_state()
    }
}

impl Session<Authed> {
    pub fn user(&self) -> &str {
        self.user.as_deref().unwrap_or_default()
    }

    pub fn send(&mut self, text: &str) -> usize {
        self.sent.push(format!("{}: {text}", self.user()));
        self.sent.len()
    }

    pub fn logout(mut self) -> Session<Connected> {
        self.user = None;
        self.into_state()
    }

    pub fn disconnect(mut self) -> Session<Disconnected> {
        self.user = None;
        self.into_state()
    }
}

fn main() {
    println!("{}", std::mem::size_of::<PhantomData<Authed>>());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("gnc14.rs");

    fn compiles_with(extra: &str) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let ok = std::process::Command::new("rustc")
            .args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings", "--out-dir"])
            .arg(&dir)
            .arg(&file)
            .output()
            .expect("не удалось запустить rustc")
            .status
            .success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }


    fn authed() -> Session<Authed> {
        match Session::new("example.org").connect().login("ann", "nna") {
            Ok(s) => s,
            Err((_, why)) => panic!("вход должен удаться: {why}"),
        }
    }

    #[test]
    fn happy_path() {
        let mut s = authed();
        assert_eq!(s.host(), "example.org");
        assert_eq!(s.user(), "ann");
        assert_eq!(s.send("hi"), 1);
        assert_eq!(s.send("again"), 2, "номера с единицы подряд");
        assert_eq!(s.history(), ["ann: hi", "ann: again"]);
    }

    #[test]
    fn wrong_password_returns_the_session_for_a_retry() {
        let connected = Session::new("h").connect();
        let Err((connected, why)) = connected.login("ann", "ann") else {
            panic!("пароль неверный, вход не должен удаться");
        };
        assert!(why.contains("ann"), "в причине имя пользователя: {why}");
        assert!(connected.login("ann", "nna").is_ok(), "после неудачи сессия возвращена и годится для повтора");
    }

    #[test]
    fn history_survives_transitions() {
        let mut s = authed();
        s.send("one");
        let c = s.logout();
        assert_eq!(c.history(), ["ann: one"], "выход не стирает историю");
        let Err((c, _)) = c.login("bob", "wrong") else { panic!("пароль неверный") };
        assert_eq!(c.history(), ["ann: one"], "неудачный вход тоже не трогает историю");
    }

    #[test]
    fn disconnect_from_both_states() {
        let d: Session<Disconnected> = authed().disconnect();
        assert_eq!(d.host(), "example.org");
        let d2: Session<Disconnected> = Session::new("x").connect().disconnect();
        assert_eq!(d2.host(), "x");
    }

    #[test]
    fn invalid_calls_do_not_compile() {
        assert!(compiles_with("fn p(s: &mut Session<Authed>) { s.send(\"x\"); }"), "send после входа доступен");
        assert!(!compiles_with("fn p(s: &mut Session<Connected>) { s.send(\"x\"); }"), "send до входа не должен собираться");
        assert!(!compiles_with("fn p(s: &mut Session<Disconnected>) { s.send(\"x\"); }"), "send без подключения тоже");
        assert!(!compiles_with("fn p(s: Session<Disconnected>) { s.login(\"a\", \"a\"); }"), "login без подключения");
        assert!(!compiles_with("fn p(s: Session<Authed>) { s.connect(); }"), "connect у вошедшей сессии");
    }
}
