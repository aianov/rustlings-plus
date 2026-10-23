use contracts::ids::UserId;
use contracts::message::Message;
use store_engine::Store;

pub fn archive(lines: &[String]) -> String {
    let mut store = Store::new();
    for line in lines {
        let Some((id, text)) = line.split_once(':') else {
            continue;
        };
        if let Some(user) = UserId::parse(id) {
            store.push(Message::new(user, text.trim()));
        }
    }
    serde_json::to_string(&serde_json::json!({ "stored": store.len(), "authors": store.authors() }))
        .unwrap_or_default()
}
