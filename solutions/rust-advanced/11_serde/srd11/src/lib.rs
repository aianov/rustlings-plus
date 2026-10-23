use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: u64,
    pub service: String,
    pub method: String,
    pub data: Value,
}

#[derive(Debug, PartialEq)]
pub enum Call {
    Register { name: String },
    CreatePost { text: String },
    Follow { user: u64 },
}

#[derive(Debug)]
pub enum DecodeError {
    BadEnvelope(serde_json::Error),
    UnknownMethod(String),
    BadData { method: String, source: serde_json::Error },
}

#[derive(Deserialize)]
struct RegisterData {
    name: String,
}

#[derive(Deserialize)]
struct CreatePostData {
    text: String,
}

#[derive(Deserialize)]
struct FollowData {
    user: u64,
}

fn data<T: DeserializeOwned>(method: &str, value: Value) -> Result<T, DecodeError> {
    serde_json::from_value(value).map_err(|source| DecodeError::BadData { method: method.to_string(), source })
}

pub fn decode(raw: &str) -> Result<(u64, Call), DecodeError> {
    let request: Request = serde_json::from_str(raw).map_err(DecodeError::BadEnvelope)?;
    let call = match request.method.as_str() {
        "users.register" => Call::Register { name: data::<RegisterData>(&request.method, request.data)?.name },
        "posts.create" => Call::CreatePost { text: data::<CreatePostData>(&request.method, request.data)?.text },
        "users.follow" => Call::Follow { user: data::<FollowData>(&request.method, request.data)?.user },
        other => return Err(DecodeError::UnknownMethod(other.to_string())),
    };
    Ok((request.id, call))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(method: &str, data: &str) -> String {
        format!(r#"{{"id":9,"service":"x","method":"{method}","data":{data}}}"#)
    }

    #[test]
    fn decodes_every_known_method() {
        let (id, call) = decode(&envelope("users.register", r#"{"name":"ira"}"#)).unwrap();
        assert_eq!((id, call), (9, Call::Register { name: "ira".into() }));
        assert_eq!(decode(&envelope("posts.create", r#"{"text":"hi"}"#)).unwrap().1, Call::CreatePost { text: "hi".into() });
        assert_eq!(decode(&envelope("users.follow", r#"{"user":7}"#)).unwrap().1, Call::Follow { user: 7 });
    }

    #[test]
    fn a_broken_envelope_is_its_own_error() {
        for raw in ["not json", r#"{"id":1}"#, r#"{"id":1,"service":"s","method":"users.register"}"#] {
            assert!(matches!(decode(raw), Err(DecodeError::BadEnvelope(_))), "{raw}");
        }
    }

    #[test]
    fn an_unknown_method_names_itself() {
        match decode(&envelope("users.explode", "{}")) {
            Err(DecodeError::UnknownMethod(m)) => assert_eq!(m, "users.explode"),
            other => panic!("ждали UnknownMethod, получили {other:?}"),
        }
    }

    #[test]
    fn data_that_does_not_fit_the_method_is_bad_data() {
        match decode(&envelope("users.register", r#"{"name":5}"#)) {
            Err(DecodeError::BadData { method, source }) => {
                assert_eq!(method, "users.register");
                assert!(source.to_string().contains("invalid type"), "{source}");
            }
            other => panic!("ждали BadData, получили {other:?}"),
        }
        assert!(matches!(decode(&envelope("users.follow", "null")), Err(DecodeError::BadData { .. })));
        assert!(matches!(decode(&envelope("posts.create", "{}")), Err(DecodeError::BadData { .. })));
    }

    #[test]
    fn extra_fields_in_data_are_tolerated() {
        let (_, call) = decode(&envelope("users.follow", r#"{"user":7,"note":"hi"}"#)).unwrap();
        assert_eq!(call, Call::Follow { user: 7 });
    }
}
