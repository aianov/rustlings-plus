// Квиз главы: лимиты и сессии шлюза.
// 1) `allow(c, ns, user, ip, now_ms)`: запрос проходит, только если есть жетон и в ведре пользователя
//    (вместимость 5, пополнение 1 в секунду), и в ведре IP (вместимость 10, 2 в секунду). Жетон списывается из
//    обоих только если оба разрешили, всё одним скриптом. Ключи `<ns>:bucket:user:<id>` и `<ns>:bucket:ip:<ip>`.
// 2) `login(c, ns, user)`: сессия на 30 минут и индекс сессий пользователя, одним обменом.
// 3) `touch(c, ns, token)`: пользователь и продление сессии до 30 минут.
// 4) `logout_all(c, ns, user)`: закрыть все сессии пользователя.
// TODO: Напиши все функции.

use course_support::redis::{Client, Error};
use uuid::Uuid;

async fn allow(c: &Client, ns: &str, user: i64, ip: &str, now_ms: i64) -> Result<bool, Error> {
    let _ = (c, ns, user, ip, now_ms);
    todo!()
}

async fn login(c: &Client, ns: &str, user: i64) -> Result<String, Error> {
    let _ = (c, ns, user, Uuid::new_v4());
    todo!()
}

async fn touch(c: &Client, ns: &str, token: &str) -> Result<Option<i64>, Error> {
    let _ = (c, ns, token);
    todo!()
}

async fn logout_all(c: &Client, ns: &str, user: i64) -> Result<(), Error> {
    let _ = (c, ns, user);
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn user_and_ip_limits_together() {
        let s = Scope::new().await;
        let c = s.client();
        let ns = s.prefix();
        let mut ann = Vec::new();
        for _ in 0..6 {
            ann.push(allow(&c, ns, 1, "10.0.0.1", 0).await.expect("лимит"));
        }
        assert_eq!(ann, [true, true, true, true, true, false], "у пользователя вместимость 5");
        let mut others = 0;
        for u in 2..=8 {
            if allow(&c, ns, u, "10.0.0.1", 0).await.expect("лимит") {
                others += 1;
            }
        }
        assert_eq!(others, 5, "у IP вместимость 10: пять уже потратила ann");
        assert!(allow(&c, ns, 2, "10.0.0.1", 1000).await.expect("лимит"), "через секунду IP пополнился на 2");
        let tokens = s.cmd(&["HGET", &s.key("bucket:user:1"), "tokens"]).await.expect("HGET").text();
        assert_eq!(tokens.as_deref(), Some("0"), "отказ из-за одного ведра не списывает жетон из другого");
    }

    #[tokio::test]
    async fn concurrent_requests() {
        let s = Scope::new().await;
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (c, ns) = (s.client(), s.prefix().to_string());
            tasks.push(tokio::spawn(async move { allow(&c, &ns, 7, "10.0.0.9", 0).await.expect("лимит") }));
        }
        let mut ok = 0;
        for t in tasks {
            if t.await.expect("задача") {
                ok += 1;
            }
        }
        assert_eq!(ok, 5, "из двадцати одновременных прошло пять");
    }

    #[tokio::test]
    async fn sessions_slide_and_die_together() {
        let s = Scope::new().await;
        let c = s.client();
        let before = c.round_trips();
        let a = login(&c, s.prefix(), 7).await.expect("вход");
        assert_eq!(c.round_trips() - before, 1, "вход одним обменом");
        let b = login(&c, s.prefix(), 7).await.expect("вход");
        s.cmd(&["EXPIRE", &s.key(&format!("session:{a}")), "5"]).await.expect("почти истекла");
        assert_eq!(touch(&c, s.prefix(), &a).await.expect("запрос"), Some(7));
        let ttl = s.cmd(&["TTL", &s.key(&format!("session:{a}"))]).await.expect("TTL").int().unwrap_or(-1);
        assert!(ttl > 1700, "активность продлевает сессию, TTL = {ttl}");
        logout_all(&c, s.prefix(), 7).await.expect("выход везде");
        assert_eq!(touch(&c, s.prefix(), &a).await.expect("запрос"), None);
        assert_eq!(touch(&c, s.prefix(), &b).await.expect("запрос"), None);
    }
}
