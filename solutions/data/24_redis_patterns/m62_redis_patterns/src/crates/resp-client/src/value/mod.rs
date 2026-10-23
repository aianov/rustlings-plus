use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Simple(String),
    Error(String),
    Integer(i64),
    Bulk(Option<Bytes>),
    Array(Option<Vec<Value>>),
}

impl Default for Value {
    fn default() -> Value {
        Value::Bulk(None)
    }
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Integer(value) => Some(*value),
            Value::Bulk(Some(bytes)) => std::str::from_utf8(bytes).ok()?.parse().ok(),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<String> {
        match self {
            Value::Simple(text) => Some(text.clone()),
            Value::Bulk(Some(bytes)) => String::from_utf8(bytes.to_vec()).ok(),
            _ => None,
        }
    }

    pub fn items(self) -> Vec<Value> {
        match self {
            Value::Array(Some(items)) => items,
            _ => Vec::new(),
        }
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Bulk(None) | Value::Array(None))
    }
}
