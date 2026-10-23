// Контекст запроса (кто арендатор, какие у пользователя роли) клонируется на каждый обработчик, а их на
// запрос десятки. Профилирование показало: на копирование строк и вектора ролей уходит больше времени, чем
// на сам ответ. Данные после создания никогда не меняются, и делить их можно сколько угодно. Клон должен
// стоить один атомарный счётчик, а не новые аллокации, при этом `Context` остаётся `Clone`, `Send` и `Sync`
// (его раздают потокам), а его методы не меняются.
// TODO: Перестрой поля `Context` так, чтобы клон ничего не копировал.

#[derive(Clone)]
pub struct Context {
    tenant: String,
    roles: Vec<String>,
}

impl Context {
    pub fn new(tenant: &str, roles: &[&str]) -> Context {
        Context { tenant: tenant.to_string(), roles: roles.iter().map(|r| r.to_string()).collect() }
    }

    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    pub fn can(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}

fn main() {
    let c = Context::new("acme", &["admin"]);
    println!("{} {}", c.tenant(), c.can("admin"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn behaves_like_before() {
        let c = Context::new("acme", &["admin", "editor"]);
        assert_eq!(c.tenant(), "acme");
        assert_eq!(c.roles(), ["admin", "editor"]);
        assert!(c.can("editor"));
        assert!(!c.can("owner"));
        let empty = Context::new("", &[]);
        assert!(empty.roles().is_empty());
    }

    #[test]
    fn clones_share_the_same_memory() {
        let a = Context::new("acme", &["admin"]);
        let b = a.clone();
        assert_eq!(a.tenant().as_ptr(), b.tenant().as_ptr(), "строка арендатора не должна копироваться при clone");
        assert_eq!(a.roles().as_ptr(), b.roles().as_ptr(), "вектор ролей тоже");
    }

    #[test]
    fn clones_can_go_to_other_threads() {
        let ctx = Context::new("acme", &["admin"]);
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let ctx = ctx.clone();
                thread::spawn(move || ctx.can("admin") && ctx.tenant() == "acme")
            })
            .collect();
        assert!(handles.into_iter().all(|h| h.join().expect("поток")), "клоны должны безопасно уходить в потоки");
    }
}
