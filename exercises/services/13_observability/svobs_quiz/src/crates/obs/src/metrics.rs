#[derive(Default)]
pub struct Registry {
    // TODO: что здесь хранится, решаешь ты.
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    pub fn inc(&self, name: &str, help: &str, labels: &[(&str, &str)]) {
        let _ = (name, help, labels);
        todo!()
    }

    pub fn gauge_add(&self, name: &str, help: &str, delta: i64) {
        let _ = (name, help, delta);
        todo!()
    }

    pub fn observe(&self, name: &str, help: &str, bounds: &[f64], labels: &[(&str, &str)], value: f64) {
        let _ = (name, help, bounds, labels, value);
        todo!()
    }

    pub fn render(&self) -> String {
        todo!()
    }
}
