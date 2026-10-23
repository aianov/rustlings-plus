use serde::de::DeserializeOwned;
use std::collections::HashMap;

pub struct Client {
    bodies: HashMap<String, String>,
}

impl Client {
    pub fn new(bodies: &[(&str, &str)]) -> Client {
        Client { bodies: bodies.iter().map(|(path, body)| (path.to_string(), body.to_string())).collect() }
    }

    fn fetch_body(&self, path: &str) -> String {
        self.bodies.get(path).cloned().unwrap_or_else(|| "null".to_string())
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, serde_json::Error> {
        let body = self.fetch_body(path);
        serde_json::from_str(&body)
    }

    pub fn get_all<T: DeserializeOwned>(&self, paths: &[&str]) -> Result<Vec<T>, serde_json::Error> {
        paths.iter().map(|path| self.get(path)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Deserialize)]
    struct Post {
        id: u64,
        title: String,
    }

    fn client() -> Client {
        Client::new(&[
            ("/post/1", r#"{"id":1,"title":"one"}"#),
            ("/post/2", r#"{"id":2,"title":"two"}"#),
            ("/ids", "[3,1,2]"),
            ("/counts", r#"{"a":1,"b":2}"#),
        ])
    }

    #[test]
    fn typed_documents() {
        assert_eq!(client().get::<Post>("/post/1").unwrap(), Post { id: 1, title: "one".into() });
        assert_eq!(client().get::<Vec<u32>>("/ids").unwrap(), vec![3, 1, 2]);
        let counts: HashMap<String, u32> = client().get("/counts").unwrap();
        assert_eq!(counts["b"], 2);
    }

    #[test]
    fn several_documents_at_once() {
        let posts: Vec<Post> = client().get_all(&["/post/2", "/post/1"]).unwrap();
        assert_eq!(posts.iter().map(|p| p.id).collect::<Vec<_>>(), vec![2, 1]);
    }

    #[test]
    fn missing_documents_and_wrong_shapes() {
        assert_eq!(client().get::<Option<u32>>("/nowhere").unwrap(), None, "нет документа: null");
        assert!(client().get::<Post>("/nowhere").is_err());
        assert!(client().get_all::<Post>(&["/post/1", "/ids"]).is_err(), "одна плохая форма портит весь набор");
    }
}
