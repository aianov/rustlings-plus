use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    parts: Vec<Bytes>,
}

pub fn cmd(name: &str) -> Cmd {
    Cmd {
        parts: vec![Bytes::copy_from_slice(name.as_bytes())],
    }
}

impl Cmd {
    pub fn arg(mut self, value: impl ToString) -> Cmd {
        self.parts.push(Bytes::from(value.to_string()));
        self
    }

    pub fn args<T: ToString>(mut self, values: impl IntoIterator<Item = T>) -> Cmd {
        for value in values {
            self.parts.push(Bytes::from(value.to_string()));
        }
        self
    }

    pub fn bytes(mut self, value: Bytes) -> Cmd {
        self.parts.push(value);
        self
    }

    pub fn parts(&self) -> &[Bytes] {
        &self.parts
    }

    pub fn name(&self) -> String {
        self.parts
            .first()
            .map(|name| String::from_utf8_lossy(name).to_uppercase())
            .unwrap_or_default()
    }
}
