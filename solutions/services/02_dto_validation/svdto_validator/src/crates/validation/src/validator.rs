use crate::rules::Rule;

#[derive(Debug, Clone, PartialEq)]
pub struct FieldError {
    pub path: String,
    pub code: &'static str,
    pub message: String,
}

#[derive(Default)]
pub struct Validator {
    errors: Vec<FieldError>,
    path: Vec<String>,
}

impl Validator {
    pub fn new() -> Validator {
        Validator::default()
    }

    fn full_path(&self, field: &str) -> String {
        let mut out = String::new();
        for part in self.path.iter().map(String::as_str).chain(std::iter::once(field)) {
            if part.is_empty() {
                continue;
            }
            if !out.is_empty() && !part.starts_with('[') {
                out.push('.');
            }
            out.push_str(part);
        }
        out
    }

    pub fn error(&mut self, field: &str, code: &'static str, message: impl Into<String>) {
        let path = self.full_path(field);
        self.errors.push(FieldError { path, code, message: message.into() });
    }

    pub fn check<T: ?Sized>(&mut self, field: &str, value: &T, rules: &[&dyn Rule<T>]) {
        if let Some((code, message)) = rules.iter().find_map(|rule| rule.check(value)) {
            self.error(field, code, message);
        }
    }

    pub fn scope(&mut self, field: &str, f: impl FnOnce(&mut Validator)) {
        self.path.push(field.to_string());
        f(self);
        self.path.pop();
    }

    pub fn each<T>(&mut self, field: &str, items: &[T], mut f: impl FnMut(&mut Validator, &T)) {
        self.path.push(field.to_string());
        for (index, item) in items.iter().enumerate() {
            self.path.push(format!("[{index}]"));
            f(self, item);
            self.path.pop();
        }
        self.path.pop();
    }

    pub fn finish(self) -> Result<(), Vec<FieldError>> {
        if self.errors.is_empty() { Ok(()) } else { Err(self.errors) }
    }
}
