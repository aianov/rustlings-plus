use chat_service::dto::text;
use chat_service::{ChatError, MAX_MESSAGE_LEN, MessageText};
use proto::social::v1::SendRequest;

fn send(raw: &str) -> SendRequest {
    SendRequest {
        peer_id: 2,
        text: raw.to_string(),
    }
}

#[test]
fn message_text_is_checked_at_the_edge() {
    assert_eq!(
        text(&send("  привет  ")).expect("текст").as_ref(),
        "привет",
        "пробелы по краям обрезаны до проверки"
    );
    let blank = text(&send(" \n ")).expect_err("пусто");
    assert_eq!(blank.pairs(), [("text", "blank")]);
    let long = text(&send(&"я".repeat(MAX_MESSAGE_LEN + 1))).expect_err("длинно");
    assert_eq!(long.pairs(), [("text", "too_long")]);
    assert!(
        MessageText::try_from("я".repeat(MAX_MESSAGE_LEN).as_str()).is_ok(),
        "предел считается в символах, а не в байтах"
    );
    assert_eq!(
        ChatError::SelfMessage.code(),
        "rejected",
        "«себе» это правило переписки, а не ошибка поля: остаётся rejected"
    );
}
