pub const HELP: &str = "\
команды:
  register <ник> <пароль> [почта]  зарегистрироваться (POST /auth/register через шлюз)
  login <ник> <пароль>       войти: access открывает WebSocket, refresh лежит в cookie
  logout                     выйти: сессия гасится, WebSocket закрывается
  logout all                 выйти на всех устройствах
  delete account             удалить аккаунт со всеми постами (сага: заморозка, профиль, посты, учётка)
  password <старый> <новый>  сменить пароль: другие устройства выйдут
  confirm                    прислать письмо для подтверждения почты (пока в лог auth-service)
  forgot <ник>               прислать письмо для сброса пароля
  find <ник>                 найти пользователя (можно и до входа)
  card <ник>                 карточка: профиль, подписчики и последние посты одним запросом
  me                         кто я (GET /users/me с Bearer-токеном)
  set name|bio <текст>       поменять имя или «о себе» (PATCH /users/me)
  clear name|bio             очистить имя или «о себе» (PATCH с null)
  avatar set <файл>          загрузить аватар (png, jpg, webp)
  avatar get <ник> [файл]    скачать аватар по подписанной ссылке из профиля, докачка по Range
  profile <ник>              профиль по ссылке (GET /users/<ник>)
  show <id>                  пост по ссылке (GET /posts/<id>)
  follow <ник>               подписаться
  mute <ник>                 скрыть автора из ленты
  post <текст>               написать пост: слова с # станут тегами, http(s):// ссылками
  like <id>                  лайкнуть пост
  comment <id> <текст>       прокомментировать пост
  delete <id>                удалить свой пост
  feed [страница [размер]]   лента
  more                       следующая страница ленты по курсору
  feed --live [сколько]      живая лента: последние посты, потом новые сами
  feed --stop                выключить живую ленту
  msg <ник> <текст>          личное сообщение: придёт на все устройства собеседника на любом шлюзе
  typing <ник>               показать собеседнику «печатает…» (не чаще раза в 3 секунды)
  online <ник>               в сети ли человек хоть на одном устройстве
  history <ник> [сколько]    переписка
  bell                       колокольчик: непрочитанные и последние уведомления
  read                       отметить все уведомления прочитанными
  trace on|off               показывать request-id каждого ответа: по нему путь запроса ищется в логах
  help                       эта справка
  quit                       выйти";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Empty,
    Help,
    Quit,
    Register {
        name: String,
        password: String,
        email: Option<String>,
    },
    Hello {
        name: String,
        password: String,
    },
    Find(String),
    Card(String),
    Me,
    Set {
        field: String,
        value: String,
    },
    Clear(String),
    Logout,
    LogoutAll,
    DeleteAccount,
    Password {
        old: String,
        new: String,
    },
    Confirm,
    Forgot(String),
    AvatarSet(String),
    AvatarGet {
        nick: String,
        file: Option<String>,
    },
    Profile(String),
    Show(String),
    Follow(String),
    Mute(String),
    Post(String),
    Like(String),
    Comment {
        post: String,
        text: String,
    },
    Delete(String),
    Feed {
        page: u64,
        size: u64,
    },
    Watch {
        recent: u64,
    },
    Unwatch,
    Msg {
        to: String,
        text: String,
    },
    History {
        with: String,
        limit: u64,
    },
    Typing(String),
    Online(String),
    Bell,
    ReadAll,
    Trace(bool),
    More,
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
        ("register", [nick, password]) => Ok(Command::Register {
            name: nick.to_string(),
            password: password.to_string(),
            email: None,
        }),
        ("register", [nick, password, email]) => Ok(Command::Register {
            name: nick.to_string(),
            password: password.to_string(),
            email: Some(email.to_string()),
        }),
        ("logout", ["all"]) => Ok(Command::LogoutAll),
        ("delete", ["account"]) => Ok(Command::DeleteAccount),
        ("password", [old, new]) => Ok(Command::Password {
            old: old.to_string(),
            new: new.to_string(),
        }),
        ("confirm", []) => Ok(Command::Confirm),
        ("forgot", [nick]) => Ok(Command::Forgot(nick.to_string())),
        ("set", [field, _, ..]) => Ok(Command::Set {
            field: profile_field(field)?,
            value: rest[field.len()..].trim().to_string(),
        }),
        ("clear", [field]) => Ok(Command::Clear(profile_field(field)?)),
        ("hello" | "login", [nick, password]) => Ok(Command::Hello {
            name: nick.to_string(),
            password: password.to_string(),
        }),
        ("logout", []) => Ok(Command::Logout),
        ("avatar", ["set", file]) => Ok(Command::AvatarSet(file.to_string())),
        ("avatar", ["get", nick]) => Ok(Command::AvatarGet {
            nick: nick.to_string(),
            file: None,
        }),
        ("avatar", ["get", nick, file]) => Ok(Command::AvatarGet {
            nick: nick.to_string(),
            file: Some(file.to_string()),
        }),
        ("find", [nick]) => Ok(Command::Find(nick.to_string())),
        ("card", [nick]) => Ok(Command::Card(nick.to_string())),
        ("me", []) => Ok(Command::Me),
        ("profile", [nick]) => Ok(Command::Profile(nick.to_string())),
        ("show", [post]) => Ok(Command::Show(post_id(post)?)),
        ("follow", [nick]) => Ok(Command::Follow(nick.to_string())),
        ("mute", [nick]) => Ok(Command::Mute(nick.to_string())),
        ("post", [_, ..]) => Ok(Command::Post(rest.to_string())),
        ("like", [post]) => Ok(Command::Like(post_id(post)?)),
        ("comment", [post, _, ..]) => Ok(Command::Comment {
            post: post_id(post)?,
            text: rest[post.len()..].trim().to_string(),
        }),
        ("delete", [post]) => Ok(Command::Delete(post_id(post)?)),
        ("feed", []) => Ok(Command::Feed { page: 1, size: 10 }),
        ("more", []) => Ok(Command::More),
        ("feed", ["--live"]) => Ok(Command::Watch { recent: 10 }),
        ("feed", ["--live", recent]) => Ok(Command::Watch {
            recent: number(recent)?,
        }),
        ("feed", ["--stop"]) => Ok(Command::Unwatch),
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
        ("typing", [nick]) => Ok(Command::Typing(nick.to_string())),
        ("online", [nick]) => Ok(Command::Online(nick.to_string())),
        ("bell", []) => Ok(Command::Bell),
        ("read", []) => Ok(Command::ReadAll),
        ("trace", ["on"]) => Ok(Command::Trace(true)),
        ("trace", ["off"]) => Ok(Command::Trace(false)),
        _ => Err(format!("не понял «{line}», help покажет команды")),
    }
}

fn profile_field(word: &str) -> Result<String, String> {
    match word {
        "name" => Ok(String::from("display_name")),
        "bio" => Ok(String::from("bio")),
        _ => Err(format!("«{word}» не поле профиля: name или bio")),
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
