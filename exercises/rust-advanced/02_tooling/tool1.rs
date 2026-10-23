// Утилита devctl управляет локальным окружением, и ей нужен разбор аргументов. Готовых парсеров мы пока
// не берём: список аргументов это обычный срез строк, а разобрать его можно руками.
// Что должно получаться:
//   devctl up [--service NAME]... [-v | --verbose]    сервисы по порядку; ни одного не указали = пустой список
//   devctl down                                       без флагов
//   devctl status [--json]
//   devctl help | --help | -h                         справка
// Флаг со значением можно писать и как `--service auth`, и как `--service=auth`; пустое значение
// (`--service=` или флаг в конце) это тоже отсутствие значения.
// Ошибки возвращаются значением, паниковать на чужом вводе нельзя.
// TODO: Реализуй `parse`. Типы и тесты менять не нужно.

#[derive(Debug, PartialEq)]
pub enum Command {
    Up { services: Vec<String>, verbose: bool },
    Down,
    Status { json: bool },
    Help,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    NoCommand,
    UnknownCommand(String),
    UnknownFlag(String),
    MissingValue(String),
}

pub fn parse(args: &[String]) -> Result<Command, ParseError> {
    todo!("разобрать аргументы после имени программы")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(command) => println!("{command:?}"),
        Err(error) => eprintln!("devctl: {error:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(line: &str) -> Result<Command, ParseError> {
        let args: Vec<String> = line.split_whitespace().map(str::to_string).collect();
        parse(&args)
    }

    fn up(services: &[&str], verbose: bool) -> Result<Command, ParseError> {
        Ok(Command::Up { services: services.iter().map(|s| s.to_string()).collect(), verbose })
    }

    #[test]
    fn no_arguments_is_an_error_not_a_panic() {
        assert_eq!(p(""), Err(ParseError::NoCommand), "пустой список аргументов это NoCommand");
    }

    #[test]
    fn simple_commands() {
        assert_eq!(p("down"), Ok(Command::Down));
        assert_eq!(p("status"), Ok(Command::Status { json: false }));
        assert_eq!(p("status --json"), Ok(Command::Status { json: true }));
    }

    #[test]
    fn help_has_three_spellings() {
        for spelling in ["help", "--help", "-h"] {
            assert_eq!(p(spelling), Ok(Command::Help), "`{spelling}` должен давать справку");
        }
    }

    #[test]
    fn up_without_flags_means_everything() {
        assert_eq!(p("up"), up(&[], false), "up без флагов: пустой список сервисов и не verbose");
    }

    #[test]
    fn up_collects_services_in_order() {
        assert_eq!(p("up --service auth --service api"), up(&["auth", "api"], false), "порядок сервисов сохраняется");
        assert_eq!(p("up --service=auth -v"), up(&["auth"], true), "форма --service=auth и короткий флаг -v");
        assert_eq!(p("up --verbose --service api"), up(&["api"], true), "длинный флаг --verbose в любом месте");
    }

    #[test]
    fn value_is_not_mistaken_for_a_flag() {
        assert_eq!(p("up --service -v"), up(&["-v"], false), "значение после --service берётся как есть, даже если оно похоже на флаг");
    }

    #[test]
    fn missing_value_is_reported() {
        assert_eq!(p("up --service"), Err(ParseError::MissingValue("--service".to_string())), "--service без значения");
        assert_eq!(p("up --service="), Err(ParseError::MissingValue("--service".to_string())), "--service= с пустым значением");
    }

    #[test]
    fn unknown_things_are_reported_with_their_names() {
        assert_eq!(p("launch"), Err(ParseError::UnknownCommand("launch".to_string())));
        assert_eq!(p("up --turbo"), Err(ParseError::UnknownFlag("--turbo".to_string())));
        assert_eq!(p("down --force"), Err(ParseError::UnknownFlag("--force".to_string())), "у down флагов нет");
        assert_eq!(p("status --yaml"), Err(ParseError::UnknownFlag("--yaml".to_string())));
    }
}
