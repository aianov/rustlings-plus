use bytes::BytesMut;

use crate::Value;

#[derive(Debug, Clone)]
pub(crate) struct Limits {
    pub max_bulk: usize,
    pub max_array: usize,
    pub max_depth: usize,
}

pub(crate) fn encode_command<S: AsRef<[u8]>>(args: &[S]) -> Vec<u8> {
    todo!()
}

pub(crate) fn decode(src: &mut BytesMut, limits: &Limits) -> Result<Option<Value>, String> {
    todo!()
}
