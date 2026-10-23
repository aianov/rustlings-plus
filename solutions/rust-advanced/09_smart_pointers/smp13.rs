use std::cell::Cell;
use std::sync::Arc;

thread_local! {
    static CLONES: Cell<usize> = const { Cell::new(0) };
}

pub fn clones() -> usize {
    CLONES.with(Cell::get)
}

#[derive(Debug, PartialEq)]
pub struct Settings {
    pub theme: String,
    pub retries: u32,
}

impl Clone for Settings {
    fn clone(&self) -> Settings {
        CLONES.with(|c| c.set(c.get() + 1));
        Settings { theme: self.theme.clone(), retries: self.retries }
    }
}

pub struct Handle {
    cfg: Arc<Settings>,
}

impl Handle {
    pub fn new(settings: Settings) -> Handle {
        Handle { cfg: Arc::new(settings) }
    }

    pub fn get(&self) -> &Settings {
        &self.cfg
    }

    pub fn update(&mut self, change: impl FnOnce(&mut Settings)) {
        change(Arc::make_mut(&mut self.cfg));
    }

    pub fn snapshot(&self) -> Handle {
        Handle { cfg: Arc::clone(&self.cfg) }
    }

    pub fn into_inner(self) -> Result<Settings, Handle> {
        Arc::try_unwrap(self.cfg).map_err(|cfg| Handle { cfg })
    }
}

fn main() {
    let mut h = Handle::new(Settings { theme: "dark".into(), retries: 3 });
    h.update(|s| s.retries = 5);
    println!("{:?}", h.get());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings { theme: "dark".to_string(), retries: 3 }
    }

    #[test]
    fn a_single_handle_is_edited_in_place() {
        let mut h = Handle::new(settings());
        let before = h.get() as *const Settings;
        h.update(|s| s.retries = 9);
        assert_eq!(h.get().retries, 9);
        assert_eq!(clones(), 0, "единственный владелец правит на месте: копировать нечего");
        assert_eq!(h.get() as *const Settings, before, "адрес не менялся");
    }

    #[test]
    fn writing_to_a_shared_snapshot_copies_once() {
        let mut a = Handle::new(settings());
        let b = a.snapshot();
        assert_eq!(clones(), 0, "snapshot делит настройки, а не копирует");
        a.update(|s| s.theme = "light".to_string());
        assert_eq!(clones(), 1, "при записи в общий снимок нужна ровно одна копия");
        assert_eq!(a.get().theme, "light");
        assert_eq!(b.get().theme, "dark", "чужой снимок остался прежним");
        a.update(|s| s.retries += 1);
        assert_eq!(clones(), 1, "теперь у a собственная копия, второй раз копировать не нужно");
        assert_eq!((a.get().retries, b.get().retries), (4, 3));
    }

    #[test]
    fn into_inner_unwraps_only_unique_handles() {
        let unique = Handle::new(settings());
        assert_eq!(unique.into_inner().ok(), Some(settings()));
        assert_eq!(clones(), 0, "достать единственные настройки не требует копирования");

        let a = Handle::new(settings());
        let b = a.snapshot();
        let Err(back) = a.into_inner() else { panic!("снимок не единственный, настройки отдавать нельзя") };
        assert_eq!(back.get(), &settings(), "вернувшийся снимок рабочий");
        assert_eq!(b.get().retries, 3);
        drop(b);
        assert!(back.into_inner().is_ok(), "когда остался один снимок, настройки достаются");
    }
}
