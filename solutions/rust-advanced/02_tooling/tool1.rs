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
    let Some((name, flags)) = args.split_first() else {
        return Err(ParseError::NoCommand);
    };
    match name.as_str() {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "down" => match flags.first() {
            Some(flag) => Err(ParseError::UnknownFlag(flag.clone())),
            None => Ok(Command::Down),
        },
        "status" => {
            let mut json = false;
            for flag in flags {
                match flag.as_str() {
                    "--json" => json = true,
                    other => return Err(ParseError::UnknownFlag(other.to_string())),
                }
            }
            Ok(Command::Status { json })
        }
        "up" => parse_up(flags),
        other => Err(ParseError::UnknownCommand(other.to_string())),
    }
}

fn parse_up(flags: &[String]) -> Result<Command, ParseError> {
    let mut services = Vec::new();
    let mut verbose = false;
    let mut iter = flags.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "-v" | "--verbose" => verbose = true,
            "--service" => {
                let value = iter.next().ok_or_else(|| ParseError::MissingValue(flag.clone()))?;
                services.push(value.clone());
            }
            other => match other.strip_prefix("--service=") {
                Some("") => return Err(ParseError::MissingValue("--service".to_string())),
                Some(value) => services.push(value.to_string()),
                None => return Err(ParseError::UnknownFlag(other.to_string())),
            },
        }
    }
    Ok(Command::Up { services, verbose })
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
