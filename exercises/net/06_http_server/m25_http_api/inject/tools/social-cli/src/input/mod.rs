pub const HELP: &str = "\
команды:
  register <ник>             зарегистрироваться (POST /auth/register)
  login <ник>                войти: токен из POST /auth/login уходит в session.hello
  bye                        выйти из сессии
  find <ник>                 найти пользователя
  profile <ник>              профиль по ссылке (GET /users/<ник>)
  show <id>                  пост по ссылке (GET /posts/<id>)
  follow <ник>               подписаться
  mute <ник>                 скрыть автора из ленты
  post <текст>               написать пост
  like <id>                  лайкнуть пост
  delete <id>                удалить свой пост
  feed [страница [размер]]   лента
  msg <ник> <текст>          личное сообщение
  history <ник> [сколько]    переписка
  help                       эта справка
  quit                       выйти";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Empty,
    Help,
    Quit,
    Register(String),
    Hello(String),
    Bye,
    Find(String),
    Profile(String),
    Show(String),
    Follow(String),
    Mute(String),
    Post(String),
    Like(String),
    Delete(String),
    Feed { page: u64, size: u64 },
    Msg { to: String, text: String },
    History { with: String, limit: u64 },
}

pub fn parse(line: &str) -> Result<Command, String> {
    let line = line.trim();
    let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let rest = rest.trim();
    let args: Vec<&str> = rest.split_whitespace().collect();
    match (word, args.as_slice()) {
        ("", _) => Ok(Command::Empty),
        ("help", []) => Ok(Command::Help),
        ("quit" | "exit", []) => Ok(Command::Quit),
        ("register", [nick]) => Ok(Command::Register(nick.to_string())),
        ("hello" | "login", [nick]) => Ok(Command::Hello(nick.to_string())),
        ("bye" | "logout", []) => Ok(Command::Bye),
        ("find", [nick]) => Ok(Command::Find(nick.to_string())),
        ("profile", [nick]) => Ok(Command::Profile(nick.to_string())),
        ("show", [post]) => Ok(Command::Show(post_id(post)?)),
        ("follow", [nick]) => Ok(Command::Follow(nick.to_string())),
        ("mute", [nick]) => Ok(Command::Mute(nick.to_string())),
        ("post", [_, ..]) => Ok(Command::Post(rest.to_string())),
        ("like", [post]) => Ok(Command::Like(post_id(post)?)),
        ("delete", [post]) => Ok(Command::Delete(post_id(post)?)),
        ("feed", []) => Ok(Command::Feed { page: 1, size: 10 }),
        ("feed", [page]) => Ok(Command::Feed {
            page: number(page)?,
            size: 10,
        }),
        ("feed", [page, size]) => Ok(Command::Feed {
            page: number(page)?,
            size: number(size)?,
        }),
        ("msg", [to, _, ..]) => {
            let text = rest[to.len()..].trim().to_string();
            Ok(Command::Msg {
                to: to.to_string(),
                text,
            })
        }
        ("history", [with]) => Ok(Command::History {
            with: with.to_string(),
            limit: 20,
        }),
        ("history", [with, limit]) => Ok(Command::History {
            with: with.to_string(),
            limit: number(limit)?,
        }),
        _ => Err(format!("не понял «{line}», help покажет команды")),
    }
}

fn number(word: &str) -> Result<u64, String> {
    word.parse().map_err(|_| format!("«{word}» не число"))
}

fn post_id(word: &str) -> Result<String, String> {
    let digits = word.strip_prefix("p_").unwrap_or(word);
    match digits.parse::<u64>() {
        Ok(number) => Ok(format!("p_{number}")),
        Err(_) => Err(format!("«{word}» не id поста, пиши число: like 3")),
    }
}
