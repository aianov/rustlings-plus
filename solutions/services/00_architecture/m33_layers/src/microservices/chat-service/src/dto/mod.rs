use proto::social::v1::Message as MessageReply;

use crate::common::Message;

pub fn message(message: &Message) -> MessageReply {
    MessageReply {
        from_id: message.from.0,
        to_id: message.to.0,
        text: message.text.clone(),
    }
}
