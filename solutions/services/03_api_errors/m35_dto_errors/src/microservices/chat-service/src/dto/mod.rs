use proto::social::v1::{Message as MessageReply, SendRequest};
use validation::{Invalid, Validator};

use crate::common::{Message, MessageText};

pub fn text(request: &SendRequest) -> Result<MessageText, Invalid> {
    let mut check = Validator::new();
    let text = check.parse::<MessageText>("text", &request.text);
    check.finish_with(text)
}

pub fn message(message: &Message) -> MessageReply {
    MessageReply {
        from_id: message.from.0,
        to_id: message.to.0,
        text: message.text.clone(),
    }
}
