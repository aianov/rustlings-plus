use std::sync::Mutex;

pub struct Store<T> {
    data: Mutex<T>,
    after_unlock: Box<dyn Fn() + Send + Sync>,
}

impl<T> Store<T> {
    pub fn new(data: T) -> Store<T> {
        Store::with_hook(data, || {})
    }

    pub fn with_hook(data: T, hook: impl Fn() + Send + Sync + 'static) -> Store<T> {
        Store { data: Mutex::new(data), after_unlock: Box::new(hook) }
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let result = f(&mut self.data.lock().unwrap());
        (self.after_unlock)();
        result
    }
}

impl<T: Default> Default for Store<T> {
    fn default() -> Self {
        Store::new(T::default())
    }
}
