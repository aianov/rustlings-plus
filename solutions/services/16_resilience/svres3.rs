
use std::time::Duration;

fn main() {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Unavailable,
    Aborted,
    Internal,
    Unknown,
    DeadlineExceeded,
    ResourceExhausted,
    InvalidArgument,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    Unauthenticated,
    FailedPrecondition,
    Cancelled,
    Unimplemented,
}

pub struct Attempt {
    pub code: Code,
    /// Можно ли безопасно выполнить этот запрос дважды (чтение, `PUT`, запрос с ключом идемпотентности).
    pub idempotent: bool,
    /// Сколько осталось до общего срока запроса.
    pub budget_left: Duration,
    /// Пауза перед следующей попыткой.
    pub pause: Duration,
}

/// Правила:
/// 1. Если после паузы не остаётся времени (`budget_left <= pause`), не повторяем ничего.
/// 2. `Unavailable`: повторяем всегда, запрос до сервера не дошёл.
/// 3. `Aborted`, `Internal`, `Unknown`, `DeadlineExceeded`: сервер мог успеть сделать часть дела, поэтому повторяем только идемпотентное.
/// 4. `ResourceExhausted`: сервер просит отступить, повтор добавит ему нагрузки. Не повторяем.
/// 5. Остальные коды говорят «запрос плохой» или «так нельзя», повтор даст то же самое. Не повторяем.
pub fn should_retry(a: &Attempt) -> bool {
    if a.budget_left <= a.pause {
        return false;
    }
    match a.code {
        Code::Unavailable => true,
        Code::Aborted | Code::Internal | Code::Unknown | Code::DeadlineExceeded => a.idempotent,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn attempt(code: Code, idempotent: bool) -> Attempt {
        Attempt { code, idempotent, budget_left: MS(1000), pause: MS(100) }
    }

    #[test]
    fn unavailable_is_always_retried() {
        assert!(should_retry(&attempt(Code::Unavailable, false)), "Unavailable значит «до сервера не дошло»: повторять безопасно даже для создания");
        assert!(should_retry(&attempt(Code::Unavailable, true)));
    }

    #[test]
    fn maybe_executed_errors_need_idempotency() {
        for code in [Code::Aborted, Code::Internal, Code::Unknown, Code::DeadlineExceeded] {
            assert!(should_retry(&attempt(code, true)), "{code:?} у идемпотентного запроса повторяем");
            assert!(!should_retry(&attempt(code, false)), "{code:?} у неидемпотентного запроса повтор может выполнить дело дважды");
        }
    }

    #[test]
    fn caller_mistakes_and_overload_are_never_retried() {
        for code in [
            Code::InvalidArgument,
            Code::NotFound,
            Code::AlreadyExists,
            Code::PermissionDenied,
            Code::Unauthenticated,
            Code::FailedPrecondition,
            Code::Cancelled,
            Code::Unimplemented,
            Code::ResourceExhausted,
        ] {
            for idem in [true, false] {
                assert!(!should_retry(&attempt(code, idem)), "{code:?} (идемпотентный: {idem}): повтор ничего не изменит или сделает хуже");
            }
        }
    }

    #[test]
    fn nothing_is_retried_without_time_left() {
        let late = Attempt { code: Code::Unavailable, idempotent: true, budget_left: MS(100), pause: MS(100) };
        assert!(!should_retry(&late), "после паузы останется ноль: попытка уже не успеет");
        let later = Attempt { code: Code::Unavailable, idempotent: true, budget_left: MS(30), pause: MS(100) };
        assert!(!should_retry(&later), "пауза длиннее остатка бюджета");
        let just = Attempt { code: Code::Unavailable, idempotent: true, budget_left: MS(101), pause: MS(100) };
        assert!(should_retry(&just), "миллисекунда на попытку ещё есть");
    }
}
