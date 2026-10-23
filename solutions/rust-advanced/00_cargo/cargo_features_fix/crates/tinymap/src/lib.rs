#[derive(Debug, Default)]
pub struct TinyMap<K, V>(Vec<(K, V)>);

impl<K: PartialEq, V> TinyMap<K, V> {
    pub fn new() -> Self {
        TinyMap(Vec::new())
    }

    pub fn bump(&mut self, key: K)
    where
        V: Default + std::ops::AddAssign<u32>,
    {
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => *v += 1,
            None => {
                let mut v = V::default();
                v += 1;
                self.0.push((key, v));
            }
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}
