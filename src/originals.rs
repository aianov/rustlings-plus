use anyhow::Result;
use std::{collections::BTreeMap, fs, path::Path};

use crate::fsutil::write_if_changed;

pub const FILE: &str = ".rustlings/originals";

pub fn hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub struct Originals {
    map: BTreeMap<String, u64>,
    existed: bool,
}

impl Originals {
    pub fn load(root: &Path) -> Self {
        let Ok(content) = fs::read_to_string(root.join(FILE)) else {
            return Self {
                map: BTreeMap::new(),
                existed: false,
            };
        };
        let map = content
            .lines()
            .filter_map(|line| {
                let (h, path) = line.split_once(' ')?;
                Some((path.to_owned(), u64::from_str_radix(h, 16).ok()?))
            })
            .collect();
        Self { map, existed: true }
    }

    pub fn existed(&self) -> bool {
        self.existed
    }

    pub fn get(&self, path: &str) -> Option<u64> {
        self.map.get(path).copied()
    }

    pub fn set(&mut self, path: &str, content: &[u8]) {
        self.map.insert(path.to_owned(), hash(content));
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let mut out = String::with_capacity(self.map.len() * 64);
        for (path, h) in &self.map {
            out.push_str(&format!("{h:016x} {path}\n"));
        }
        write_if_changed(root.join(FILE), out.as_bytes())?;
        Ok(())
    }
}

pub fn record<'a>(root: &Path, files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Result<()> {
    let mut originals = Originals::load(root);
    for (path, content) in files {
        originals.set(path, content);
    }
    originals.save(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_and_roundtrip() {
        assert_eq!(hash(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(hash(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_ne!(hash(b"ab"), hash(b"ba"));
        let root = std::env::temp_dir().join(format!("rustlings-orig-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut o = Originals::load(&root);
        assert!(!o.existed());
        o.set("exercises/a b.rs", b"x");
        o.save(&root).unwrap();
        let o = Originals::load(&root);
        assert!(o.existed());
        assert_eq!(o.get("exercises/a b.rs"), Some(hash(b"x")));
        assert_eq!(o.get("missing"), None);
        fs::remove_dir_all(&root).unwrap();
    }
}
