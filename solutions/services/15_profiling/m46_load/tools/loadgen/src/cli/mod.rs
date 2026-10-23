use std::time::Duration;

use crate::ramp::Ramp;
use crate::scenario::Scenario;

pub const USAGE: &str = "\
loadgen: нагрузка на шлюз соцсети
  --gateway <адрес>      шлюз (по умолчанию GATEWAY_HTTP_ADDR или 127.0.0.1:7880)
  --model closed|open    закрытая модель (пользователи по кругу) или открытая (прибытия в секунду)
  --users <n>            виртуальные пользователи (закрытая) или исполнители (открытая), 10
  --rate <в секунду>     прибытия в секунду для открытой модели, 50
  --duration <секунды>   длина прогона, 10
  --scenario <шаги>      через запятую из login, post, feed, message; по умолчанию post,feed,message,feed
  --ramp                 ступени: открытая модель с удвоением, пока p99 и ошибки в пределах
  --p99-ms <мс>          предел p99 для ступеней, 250
  --max-errors <доля>    предел доли ошибок для ступеней, 0.01
  --save <файл>          сохранить отчёт JSON
  --compare <файл>       сравнить с отчётом прошлого прогона";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Closed,
    Open,
    Ramp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub gateway: String,
    pub mode: Mode,
    pub users: usize,
    pub rate: f64,
    pub duration: Duration,
    pub scenario: Scenario,
    pub ramp: Ramp,
    pub save: Option<String>,
    pub compare: Option<String>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            gateway: String::from("127.0.0.1:7880"),
            mode: Mode::Closed,
            users: 10,
            rate: 50.0,
            duration: Duration::from_secs(10),
            scenario: Scenario::mixed(),
            ramp: Ramp::default(),
            save: None,
            compare: None,
        }
    }
}

fn number<T: std::str::FromStr>(flag: &str, value: Option<&String>) -> Result<T, String> {
    let value = value.ok_or_else(|| format!("{flag}: нужно значение"))?;
    value
        .parse()
        .map_err(|_| format!("{flag}: «{value}» не число"))
}

pub fn parse_args(args: &[String], gateway: Option<String>) -> Result<Options, String> {
    let mut options = Options {
        gateway: gateway.unwrap_or_else(|| Options::default().gateway),
        ..Options::default()
    };
    let mut at = 0;
    while at < args.len() {
        let flag = args[at].as_str();
        let value = args.get(at + 1);
        let mut used = 2;
        match flag {
            "--gateway" => options.gateway = value.cloned().ok_or("--gateway: нужен адрес")?,
            "--model" => {
                options.mode = match value.map(String::as_str) {
                    Some("closed") => Mode::Closed,
                    Some("open") => Mode::Open,
                    other => return Err(format!("--model: closed или open, а не {other:?}")),
                }
            }
            "--users" => options.users = number(flag, value)?,
            "--rate" => options.rate = number(flag, value)?,
            "--duration" => options.duration = Duration::from_secs_f64(number(flag, value)?),
            "--scenario" => {
                options.scenario = Scenario::parse(value.ok_or("--scenario: нужны шаги")?)?;
            }
            "--ramp" => {
                options.mode = Mode::Ramp;
                used = 1;
            }
            "--p99-ms" => options.ramp.p99 = Duration::from_millis(number(flag, value)?),
            "--max-errors" => options.ramp.errors = number(flag, value)?,
            "--save" => options.save = value.cloned(),
            "--compare" => options.compare = value.cloned(),
            other => return Err(format!("не знаю флага {other}\n{USAGE}")),
        }
        at += used;
    }
    if options.users == 0 || options.rate <= 0.0 || options.duration.is_zero() {
        return Err(String::from(
            "пользователи, частота и длительность должны быть больше нуля",
        ));
    }
    Ok(options)
}
