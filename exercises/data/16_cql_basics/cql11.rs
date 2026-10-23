// У сообщения появились теги, реакции и вложения. Вложение это пара «тип и адрес», для неё заведён свой
// тип (UDT). Схема ниже не создаётся: Scylla ругается на вложения. Почини её, сохранив состав: tags это
// множество строк, reactions это словарь «реакция и сколько раз», attachments это список вложений.

fn schema() -> &'static str {
    "CREATE TYPE attachment (kind text, url text);
     CREATE TABLE messages (chat_id int, sent_at bigint, body text, tags set<text>, reactions map<text, int>, attachments list<attachment>, PRIMARY KEY (chat_id, sent_at))"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    #[tokio::test]
    async fn collections_live_and_update() {
        let ks = TestKs::with_cql(schema()).await;
        ks.run("INSERT INTO messages (chat_id, sent_at, body, tags, reactions, attachments) VALUES (1, 100, 'смотри', {'rust', 'async'}, {'like': 3}, [{kind: 'image', url: 'a.png'}])").await;
        ks.run("UPDATE messages SET tags = tags + {'tokio'}, reactions['fire'] = 1, attachments = attachments + [{kind: 'file', url: 'b.txt'}] WHERE chat_id = 1 AND sent_at = 100").await;
        let t = ks.table("SELECT tags, reactions, attachments FROM messages WHERE chat_id = 1 AND sent_at = 100").await;
        assert_eq!(t.cell(0, 0), "{async, rust, tokio}", "теги это множество строк\n{t}");
        assert_eq!(t.cell(0, 1), "{fire: 1, like: 3}", "реакции это словарь с числами\n{t}");
        assert_eq!(t.cell(0, 2), "[{kind: image, url: a.png}, {kind: file, url: b.txt}]", "вложения это список, новые в конце\n{t}");
    }
}
