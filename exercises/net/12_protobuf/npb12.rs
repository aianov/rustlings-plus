// Очередь заданий принимает задачи от нескольких сервисов, и у каждой есть приоритет-перечисление. Недавно соседняя команда
// выкатила новую версию схемы с четвёртым приоритетом, а наш сервис остался на старой. Теперь в очередь приходят задачи
// с числом, которого наше перечисление не знает. В prost поле-перечисление хранится как обычное i32, и такие значения
// не теряются, пока с ними обращаются аккуратно. TODO: Допиши classify и escalate (Unspecified → Low → High, High остаётся).
// Неизвестное значение приоритета повышать нельзя и терять нельзя: задача проходит через нас дальше нетронутой.

#![allow(dead_code)]
use prost::Message;

#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
enum Priority {
    Unspecified = 0,
    Low = 1,
    High = 2,
}

#[derive(Clone, PartialEq, Message)]
struct Job {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(enumeration = "Priority", tag = "2")]
    priority: i32,
}

#[derive(Debug, PartialEq)]
enum Class {
    Known(Priority),
    Unknown(i32),
}

fn classify(job: &Job) -> Class {
    let _ = job;
    todo!()
}

fn escalate(job: &Job) -> Job {
    let _ = job;
    todo!()
}

fn forward(bytes: &[u8]) -> Result<Vec<u8>, prost::DecodeError> {
    let job = Job::decode(bytes)?;
    Ok(escalate(&job).encode_to_vec())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(name: &str, priority: i32) -> Job {
        Job { name: name.into(), priority }
    }

    #[test]
    fn known_values_are_classified() {
        assert_eq!(classify(&job("a", 0)), Class::Known(Priority::Unspecified));
        assert_eq!(classify(&job("a", 1)), Class::Known(Priority::Low));
        assert_eq!(classify(&job("a", 2)), Class::Known(Priority::High));
    }

    #[test]
    fn unknown_values_are_not_mistaken_for_defaults() {
        assert_eq!(classify(&job("a", 9)), Class::Unknown(9), "неизвестное значение нельзя молча считать значением по умолчанию");
        assert_eq!(classify(&job("a", -1)), Class::Unknown(-1));
        assert_eq!(classify(&job("a", 3)), Class::Unknown(3));
    }

    #[test]
    fn escalation_ladder() {
        assert_eq!(escalate(&job("a", 0)).priority, 1);
        assert_eq!(escalate(&job("a", 1)).priority, 2);
        assert_eq!(escalate(&job("a", 2)).priority, 2);
        assert_eq!(escalate(&job("keep", 1)).name, "keep");
    }

    #[test]
    fn unknown_priority_survives_a_trip_through_us() {
        let from_newer_service = [0x0a, 0x01, b'x', 0x10, 0x09];
        let forwarded = forward(&from_newer_service).unwrap();
        assert_eq!(Job::decode(&forwarded[..]).unwrap(), job("x", 9), "значение 9 должно дойти до следующего сервиса");
        assert_eq!(forwarded, from_newer_service.to_vec());
    }

    #[test]
    fn negative_unknown_values_survive_too() {
        let mut bytes = vec![0x10];
        bytes.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]);
        let forwarded = forward(&bytes).unwrap();
        assert_eq!(Job::decode(&forwarded[..]).unwrap().priority, -1);
    }

    #[test]
    fn default_priority_is_absent_on_the_wire_until_escalated() {
        assert!(job("", 0).encode_to_vec().is_empty());
        assert_eq!(forward(&[]).unwrap(), vec![0x10, 0x01], "Unspecified повышается до Low, и теперь значение надо писать");
    }
}
