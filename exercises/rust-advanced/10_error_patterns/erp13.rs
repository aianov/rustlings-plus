// Лимиты запросов задаются строкой вида `5/s`, `120/m`, `10/h`. Парсер лежит в библиотеке, которую зовут и
// из настроек, и из HTTP-заголовка, то есть данные приходят снаружи. Опечатка в настройках
// роняет весь процесс паникой, а при падении рабочего потока паника улетает дальше вместо того,
// чтобы стать обычным сообщением.
// TODO: Сделай так, чтобы любой плохой ввод давал `Err(RateError::…)`, а паника рабочей функции
// возвращалась из `run_isolated` как `Err` с её текстом. Паника уместна только для нарушенных
// внутренних инвариантов, а не для чужих данных.
use std::thread;

#[derive(Debug, PartialEq)]
pub struct Rate {
    pub count: u32,
    pub per_seconds: u32,
}

#[derive(Debug, PartialEq)]
pub enum RateError {
    Empty,
    NoSlash,
    BadCount(String),
    BadUnit(String),
    ZeroCount,
}

pub fn parse_rate(text: &str) -> Result<Rate, RateError> {
    let (count, unit) = text.split_once('/').unwrap();
    let count: u32 = count.parse().unwrap();
    assert!(count > 0);
    let per_seconds = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        _ => panic!("unknown unit"),
    };
    Ok(Rate { count, per_seconds })
}

pub fn run_isolated<T, F>(job: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    Ok(thread::spawn(job).join().unwrap())
}

fn main() {
    println!("{:?}", parse_rate("5/s"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_rates() {
        assert_eq!(parse_rate("5/s"), Ok(Rate { count: 5, per_seconds: 1 }));
        assert_eq!(parse_rate("120/m"), Ok(Rate { count: 120, per_seconds: 60 }));
        assert_eq!(parse_rate("10/h"), Ok(Rate { count: 10, per_seconds: 3600 }));
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert_eq!(parse_rate(""), Err(RateError::Empty));
        assert_eq!(parse_rate("5"), Err(RateError::NoSlash), "без слэша: ошибка, а не unwrap на None");
        assert_eq!(parse_rate("x/s"), Err(RateError::BadCount("x".into())));
        assert_eq!(parse_rate("-3/s"), Err(RateError::BadCount("-3".into())));
        assert_eq!(parse_rate("0/s"), Err(RateError::ZeroCount), "ноль запросов это ошибка настройки, а не assert");
        assert_eq!(parse_rate("5/d"), Err(RateError::BadUnit("d".into())));
        assert_eq!(parse_rate("5/"), Err(RateError::BadUnit(String::new())));
    }

    #[test]
    fn a_worker_returns_its_value() {
        assert_eq!(run_isolated(|| 40 + 2), Ok(42));
    }

    #[test]
    fn a_panic_with_a_literal_becomes_err() {
        let got = run_isolated(|| -> u32 { panic!("boom") });
        assert_eq!(got, Err("boom".to_string()));
    }

    #[test]
    fn a_panic_with_a_formatted_message_becomes_err() {
        let n = 7;
        let got = run_isolated(move || -> u32 { panic!("bad item {n}") });
        assert_eq!(got, Err("bad item 7".to_string()), "panic! с форматом несёт String, а не &str");
    }

    #[test]
    fn an_odd_payload_gets_a_default_text() {
        let got = run_isolated(|| -> u32 { std::panic::panic_any(5_u8) });
        assert_eq!(got, Err("unknown panic".to_string()));
    }
}
