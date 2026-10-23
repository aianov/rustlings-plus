use scylla::statement::{Consistency, SerialConsistency};

#[derive(Debug, Clone, Copy)]
enum Op {
    SendMessage,
    ReadHistory,
    Typing,
    Presence,
    DeleteAccount,
}

fn level(op: Op) -> Consistency {
    match op {
        Op::SendMessage | Op::ReadHistory => Consistency::Quorum,
        Op::Typing | Op::Presence => Consistency::One,
        Op::DeleteAccount => Consistency::All,
    }
}

fn serial_level() -> SerialConsistency {
    SerialConsistency::Serial
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_the_rules() {
        assert_eq!(level(Op::SendMessage), Consistency::Quorum, "отправка с QUORUM");
        assert_eq!(level(Op::ReadHistory), Consistency::Quorum, "чтение с QUORUM: вместе с записью видит подтверждённое");
        assert_eq!(level(Op::Typing), Consistency::One, "«печатает» живёт секунды: ONE");
        assert_eq!(level(Op::Presence), Consistency::One, "онлайн тоже ONE");
        assert_eq!(level(Op::DeleteAccount), Consistency::All, "удаление аккаунта на все реплики");
        assert_eq!(serial_level(), SerialConsistency::Serial, "фаза проверки LWT с SERIAL");
    }
}
