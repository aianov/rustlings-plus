use crate::{Options, Result, Value};

#[derive(Clone)]
pub struct Client {
    _private: (),
}

impl Client {
    pub async fn connect(url: &str) -> Result<Client> {
        todo!()
    }

    pub async fn connect_with(url: &str, options: Options) -> Result<Client> {
        todo!()
    }

    pub async fn call<S: AsRef<[u8]>>(&self, args: &[S]) -> Result<Value> {
        todo!()
    }

    pub async fn pipeline(&self, cmds: &[Vec<String>]) -> Result<Vec<Value>> {
        todo!()
    }

    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        todo!()
    }

    pub async fn set(&self, key: &str, value: &[u8]) -> Result<()> {
        todo!()
    }

    pub async fn set_ex(&self, key: &str, value: &[u8], seconds: u64) -> Result<()> {
        todo!()
    }

    pub async fn del(&self, key: &str) -> Result<bool> {
        todo!()
    }

    pub async fn incr(&self, key: &str) -> Result<i64> {
        todo!()
    }

    pub async fn expire(&self, key: &str, seconds: u64) -> Result<bool> {
        todo!()
    }

    pub async fn ttl(&self, key: &str) -> Result<i64> {
        todo!()
    }

    pub async fn blpop(&self, key: &str, timeout_secs: u32) -> Result<Option<(String, Vec<u8>)>> {
        todo!()
    }
}
