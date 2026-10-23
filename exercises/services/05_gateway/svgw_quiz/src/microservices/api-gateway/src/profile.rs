use crate::backend::Backend;
use crate::types::{GwError, Metadata};
use serde_json::{Value, json};
use std::time::Duration;

pub const OPTIONAL_BUDGET: Duration = Duration::from_millis(150);

pub async fn profile(backend: &dyn Backend, id: u64, meta: &Metadata) -> Result<Value, GwError> {
    todo!()
}
