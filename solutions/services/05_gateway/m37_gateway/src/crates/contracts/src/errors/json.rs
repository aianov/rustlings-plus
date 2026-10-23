use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::error::Category;

use super::{Detail, ErrorBody, ErrorCode};

fn quoted(text: &str, after: &str) -> Option<String> {
    let rest = text.split_once(after)?.1;
    let rest = rest.strip_prefix('`')?;
    Some(rest[..rest.find('`')?].to_string())
}

fn shape(error: &serde_json::Error) -> Detail {
    let text = error.to_string();
    let detail = |field: String, code: &str, message: &str| Detail {
        field,
        code: code.to_string(),
        message: message.to_string(),
    };
    if let Some(field) = quoted(&text, "missing field ") {
        return detail(field, "missing", "поле обязательно");
    }
    if let Some(field) = quoted(&text, "unknown field ") {
        return detail(field, "unknown_field", "такого поля нет");
    }
    detail(String::new(), "wrong_type", "значение не того типа")
}

pub fn json_error(error: &serde_json::Error) -> ErrorBody {
    match error.classify() {
        Category::Eof => ErrorBody::public(ErrorCode::Truncated),
        Category::Syntax | Category::Io => ErrorBody::public(ErrorCode::Syntax),
        Category::Data => ErrorBody {
            details: vec![shape(error)],
            ..ErrorBody::public(ErrorCode::Validation)
        },
    }
}

pub fn parse_body<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ErrorBody> {
    serde_json::from_slice(bytes).map_err(|error| json_error(&error))
}

pub fn read_data<T: DeserializeOwned>(method: &str, data: Value) -> Result<T, ErrorBody> {
    T::deserialize(data).map_err(|error| {
        let detail = shape(&error);
        ErrorBody {
            details: (!detail.field.is_empty())
                .then_some(detail)
                .into_iter()
                .collect(),
            ..ErrorBody::new(
                ErrorCode::BadData,
                format!("data не подходит к методу {method}"),
            )
        }
    })
}
