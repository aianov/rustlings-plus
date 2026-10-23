#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers {
    items: Vec<(String, String)>,
}

impl Headers {
    pub fn new() -> Headers {
        Headers::default()
    }

    pub fn append(&mut self, name: &str, value: &str) {
        self.items.push((name.to_string(), value.to_string()));
    }

    pub fn set(&mut self, name: &str, value: &str) {
        self.remove(name);
        self.append(name, value);
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|(own, _)| own.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn get_all(&self, name: &str) -> Vec<&str> {
        self.items
            .iter()
            .filter(|(own, _)| own.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn remove(&mut self, name: &str) -> usize {
        let before = self.items.len();
        self.items
            .retain(|(own, _)| !own.eq_ignore_ascii_case(name));
        before - self.items.len()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.items
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }
}
