// Схема сообщения меняется, а сервисы выкатываются не одновременно: часть читает старой версией, часть новой. Какие правки
// безопасны, а какие превращают чужие байты в мусор или в тихо неверные числа? Номера полей навсегда привязаны к смыслу,
// а имена на проводе вообще не передаются. Нужен проверяющий: он сравнивает старую схему с новой и перечисляет проблемы,
// по одной на номер, по возрастанию номеров. TODO: Допиши breaking_changes. Правила совместимости лежат в README главы.

#![allow(dead_code)]

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Int32,
    Int64,
    Uint32,
    Uint64,
    Bool,
    Enum,
    Sint32,
    Sint64,
    Fixed32,
    Sfixed32,
    Fixed64,
    Sfixed64,
    Float,
    Double,
    String,
    Bytes,
    Message,
}

struct FieldDef {
    number: u32,
    name: &'static str,
    kind: Kind,
}

struct Schema {
    fields: Vec<FieldDef>,
    reserved: Vec<u32>,
}

#[derive(Debug, PartialEq)]
enum Problem {
    WireTypeChanged { number: u32 },
    EncodingChanged { number: u32 },
    RemovedWithoutReserve { number: u32 },
    ReservedNumberReused { number: u32 },
    FieldInReserved { number: u32 },
    ReservationDropped { number: u32 },
}

fn f(number: u32, name: &'static str, kind: Kind) -> FieldDef {
    FieldDef { number, name, kind }
}

#[derive(PartialEq)]
enum Wire {
    Varint,
    I32,
    I64,
    Len,
}

fn wire(kind: Kind) -> Wire {
    match kind {
        Kind::Int32 | Kind::Int64 | Kind::Uint32 | Kind::Uint64 | Kind::Bool | Kind::Enum | Kind::Sint32 | Kind::Sint64 => Wire::Varint,
        Kind::Fixed32 | Kind::Sfixed32 | Kind::Float => Wire::I32,
        Kind::Fixed64 | Kind::Sfixed64 | Kind::Double => Wire::I64,
        Kind::String | Kind::Bytes | Kind::Message => Wire::Len,
    }
}

fn compatible(a: Kind, b: Kind) -> bool {
    use Kind::*;
    if a == b {
        return true;
    }
    let plain = |k: Kind| matches!(k, Int32 | Int64 | Uint32 | Uint64 | Bool | Enum);
    match (a, b) {
        _ if plain(a) && plain(b) => true,
        (Sint32 | Sint64, Sint32 | Sint64) => true,
        (Fixed32 | Sfixed32, Fixed32 | Sfixed32) => true,
        (Fixed64 | Sfixed64, Fixed64 | Sfixed64) => true,
        (String | Message, Bytes) | (Bytes, String | Message) => true,
        _ => false,
    }
}

fn breaking_changes(old: &Schema, new: &Schema) -> Vec<Problem> {
    let mut numbers: Vec<u32> = old.fields.iter().chain(new.fields.iter()).map(|f| f.number).chain(old.reserved.iter().copied()).chain(new.reserved.iter().copied()).collect();
    numbers.sort_unstable();
    numbers.dedup();
    let mut problems = Vec::new();
    for number in numbers {
        let before = old.fields.iter().find(|f| f.number == number);
        let after = new.fields.iter().find(|f| f.number == number);
        let reserved_now = new.reserved.contains(&number);
        let reserved_before = old.reserved.contains(&number);
        let problem = match (before, after) {
            (Some(_), Some(_)) if reserved_now => Some(Problem::FieldInReserved { number }),
            (Some(b), Some(a)) if compatible(b.kind, a.kind) => None,
            (Some(b), Some(a)) if wire(b.kind) != wire(a.kind) => Some(Problem::WireTypeChanged { number }),
            (Some(_), Some(_)) => Some(Problem::EncodingChanged { number }),
            (Some(_), None) if !reserved_now => Some(Problem::RemovedWithoutReserve { number }),
            (None, Some(_)) if reserved_now => Some(Problem::FieldInReserved { number }),
            (None, Some(_)) if reserved_before => Some(Problem::ReservedNumberReused { number }),
            (None, None) if reserved_before && !reserved_now => Some(Problem::ReservationDropped { number }),
            _ => None,
        };
        problems.extend(problem);
    }
    problems
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use Kind::*;

    fn schema(fields: Vec<FieldDef>, reserved: Vec<u32>) -> Schema {
        Schema { fields, reserved }
    }

    fn base() -> Schema {
        schema(vec![f(1, "id", Uint64), f(2, "name", String), f(3, "age", Int32), f(4, "legacy", Bool)], vec![])
    }

    #[test]
    fn identical_and_renamed_are_safe() {
        assert_eq!(breaking_changes(&base(), &base()), vec![]);
        let renamed = schema(vec![f(1, "user_id", Uint64), f(2, "full_name", String), f(3, "age", Int32), f(4, "legacy", Bool)], vec![]);
        assert_eq!(breaking_changes(&base(), &renamed), vec![], "имён на проводе нет, переименование безопасно");
    }

    #[test]
    fn adding_fields_is_safe() {
        let mut next = base();
        next.fields.push(f(5, "email", String));
        next.fields.push(f(6, "tags", Message));
        assert_eq!(breaking_changes(&base(), &next), vec![]);
    }

    #[test]
    fn removing_requires_a_reservation() {
        let without = schema(vec![f(1, "id", Uint64), f(2, "name", String), f(3, "age", Int32)], vec![]);
        assert_eq!(breaking_changes(&base(), &without), vec![Problem::RemovedWithoutReserve { number: 4 }], "удалили поле и не зарезервировали номер: его кто-то займёт");
        let reserved = schema(vec![f(1, "id", Uint64), f(2, "name", String), f(3, "age", Int32)], vec![4]);
        assert_eq!(breaking_changes(&base(), &reserved), vec![]);
    }

    #[test]
    fn compatible_type_changes() {
        for (from, to) in [(Int32, Int64), (Int32, Uint32), (Int32, Bool), (Uint64, Int32), (Enum, Int32), (Sint32, Sint64), (Fixed32, Sfixed32), (Sfixed64, Fixed64), (String, Bytes), (Bytes, Message), (Message, Bytes)] {
            let old = schema(vec![f(1, "x", from)], vec![]);
            let new = schema(vec![f(1, "x", to)], vec![]);
            assert_eq!(breaking_changes(&old, &new), vec![], "{from:?} в {to:?} безопасно");
        }
    }

    #[test]
    fn same_wire_type_but_different_encoding() {
        for (from, to) in [(Int32, Sint32), (Sint64, Int64), (Float, Fixed32), (Double, Fixed64), (String, Message), (Message, String), (Sint32, Bool)] {
            let old = schema(vec![f(1, "x", from)], vec![]);
            let new = schema(vec![f(1, "x", to)], vec![]);
            assert_eq!(breaking_changes(&old, &new), vec![Problem::EncodingChanged { number: 1 }], "{from:?} в {to:?}: байты читаются, но значения неверные");
        }
    }

    #[test]
    fn different_wire_type_is_always_broken() {
        for (from, to) in [(Int32, Fixed32), (Int32, String), (Double, Float), (String, Int64), (Fixed64, Fixed32), (Bool, Message)] {
            let old = schema(vec![f(1, "x", from)], vec![]);
            let new = schema(vec![f(1, "x", to)], vec![]);
            assert_eq!(breaking_changes(&old, &new), vec![Problem::WireTypeChanged { number: 1 }], "{from:?} в {to:?}");
        }
    }

    #[test]
    fn reserved_numbers_must_stay_unused() {
        let old = schema(vec![f(1, "id", Uint64)], vec![2, 3]);
        let reuse = schema(vec![f(1, "id", Uint64), f(2, "email", String)], vec![2, 3]);
        assert_eq!(breaking_changes(&old, &reuse), vec![Problem::FieldInReserved { number: 2 }], "поле объявлено на зарезервированном номере");
        let reuse_unreserved = schema(vec![f(1, "id", Uint64), f(3, "avatar", Bytes)], vec![2]);
        assert_eq!(
            breaking_changes(&old, &reuse_unreserved),
            vec![Problem::ReservedNumberReused { number: 3 }],
            "резервирование сняли и тут же заняли номер: старые байты с этим номером теперь читаются как другое поле"
        );
        let dropped = schema(vec![f(1, "id", Uint64)], vec![2]);
        assert_eq!(breaking_changes(&old, &dropped), vec![Problem::ReservationDropped { number: 3 }]);
    }

    #[test]
    fn several_problems_come_sorted_by_number() {
        let old = schema(vec![f(1, "a", Int32), f(2, "b", String), f(3, "c", Double), f(4, "d", Bool)], vec![9]);
        let new = schema(vec![f(1, "a", Sint32), f(3, "c", Float), f(7, "z", Int32)], vec![9]);
        assert_eq!(
            breaking_changes(&old, &new),
            vec![Problem::EncodingChanged { number: 1 }, Problem::RemovedWithoutReserve { number: 2 }, Problem::WireTypeChanged { number: 3 }, Problem::RemovedWithoutReserve { number: 4 }]
        );
    }
}
