// feed-service собирает ленту из событий `post.created`. Чтобы показать имя автора, на каждый пост он ходит в
// user-service. В час пик user-service тормозит, лента встаёт, а когда он лёг, потребитель перестал обрабатывать
// вообще всё. Цель: событие несёт всё, что нужно ленте для показа, и feed_item не вызывает никого. Только нужное:
// почта и прочие личные поля автора в событие не попадают. События, записанные до этой правки, без имени
// автора по-прежнему читаются: вместо имени показывается идентификатор.

use serde_json::{Value, json};

fn main() {}

pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
    pub avatar: Option<String>,
}

pub struct Post {
    pub id: String,
    pub text: String,
}

#[derive(Debug, PartialEq)]
pub struct FeedItem {
    pub post_id: String,
    pub author_name: String,
    pub author_avatar: Option<String>,
    pub text: String,
}

pub trait UserDirectory {
    fn get(&self, id: &str) -> Result<User, String>;
}

pub fn post_created_payload(post: &Post, author: &User) -> Value {
    json!({"post_id": post.id, "author_id": author.id, "text": post.text})
}

pub fn feed_item(directory: &dyn UserDirectory, payload: &Value) -> Result<FeedItem, String> {
    let text = |key: &str| payload.get(key).and_then(Value::as_str).map(str::to_string).ok_or(format!("в событии нет поля {key}"));
    let author = directory.get(&text("author_id")?)?;
    Ok(FeedItem { post_id: text("post_id")?, author_name: author.name, author_avatar: author.avatar, text: text("text")? })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> User {
        User { id: "u1".into(), name: "Alice".into(), email: "alice@example.com".into(), avatar: Some("a.png".into()) }
    }

    fn post() -> Post {
        Post { id: "p1".into(), text: "первый пост".into() }
    }

    struct Down;

    impl UserDirectory for Down {
        fn get(&self, _id: &str) -> Result<User, String> {
            Err("user-service недоступен".into())
        }
    }

    fn read(payload: &Value) -> Result<FeedItem, String> {
        feed_item(&Down, payload)
    }

    #[test]
    fn the_feed_item_is_built_from_the_event_alone() {
        let item = read(&post_created_payload(&post(), &alice())).expect("user-service лежит, а событие должно читаться");
        assert_eq!(
            item,
            FeedItem { post_id: "p1".into(), author_name: "Alice".into(), author_avatar: Some("a.png".into()), text: "первый пост".into() },
            "в ленте имя, аватар и текст прямо из события"
        );
    }

    #[test]
    fn personal_data_does_not_leave_the_owner_service() {
        let payload = post_created_payload(&post(), &alice()).to_string();
        assert!(!payload.contains("alice@example.com"), "почта автора не нужна ленте и не должна лететь всем подписчикам события: {payload}");
    }

    #[test]
    fn an_author_without_an_avatar_is_fine() {
        let mut user = alice();
        user.avatar = None;
        let item = read(&post_created_payload(&post(), &user)).unwrap();
        assert_eq!(item.author_avatar, None, "нет аватара значит None, а не ошибка");
    }

    #[test]
    fn events_written_before_the_change_are_still_readable() {
        let old = json!({"post_id": "p0", "author_id": "u9", "text": "старый пост"});
        let item = read(&old).expect("в журнале лежат события без имени автора");
        assert_eq!(item.author_name, "u9", "вместо имени показываем идентификатор");
        assert_eq!(item.author_avatar, None, "аватара нет");
    }

    #[test]
    fn an_event_without_the_essentials_is_an_error() {
        assert!(read(&json!({"author_id": "u1", "text": "x"})).is_err(), "нет post_id");
        assert!(read(&json!({"post_id": "p1", "author_id": "u1"})).is_err(), "нет текста");
    }
}
