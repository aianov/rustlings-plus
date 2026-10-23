use course_support::redis::{Client, Error};

fn name_key(ns: &str, user: i64) -> String {
    format!("{ns}:user:{user}:name")
}

async fn save_name(c: &Client, ns: &str, user: i64, name: &str) -> Result<(), Error> {
    c.cmd(&["SET", &name_key(ns, user), name]).await?;
    Ok(())
}

async fn load_name(c: &Client, ns: &str, user: i64) -> Result<Option<String>, Error> {
    Ok(c.cmd(&["GET", &name_key(ns, user)]).await?.text())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn each_user_has_own_key() {
        let s = Scope::new().await;
        let c = s.client();
        save_name(&c, s.prefix(), 1, "ann").await.expect("SET");
        save_name(&c, s.prefix(), 2, "bob").await.expect("SET");
        assert_eq!(load_name(&c, s.prefix(), 1).await.expect("GET").as_deref(), Some("ann"), "имя пользователя 1 не затёрто вторым");
        assert_eq!(load_name(&c, s.prefix(), 2).await.expect("GET").as_deref(), Some("bob"));
        assert_eq!(load_name(&c, s.prefix(), 3).await.expect("GET"), None, "нет ключа это None");
        assert_eq!(name_key("svc", 7), "svc:user:7:name", "схема ключа <ns>:user:<id>:name");
    }
}
