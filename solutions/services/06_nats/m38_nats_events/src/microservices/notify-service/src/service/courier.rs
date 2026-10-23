use crate::common::Notice;
use crate::repository::BoxFuture;

pub trait Courier: Send + Sync {
    fn deliver(&self, notice: Notice, request_id: Option<String>) -> BoxFuture<'_, ()>;
}

pub struct Quiet;

impl Courier for Quiet {
    fn deliver(&self, _: Notice, _: Option<String>) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}
