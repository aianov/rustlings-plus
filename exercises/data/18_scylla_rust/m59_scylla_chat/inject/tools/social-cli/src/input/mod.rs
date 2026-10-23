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
  history <ник> [сколько] [курсор]
                             переписка страницами, старее по курсору
  chats                      чаты, самые свежие первыми
  bell                       колокольчик: непрочитанные и последние уведомления
  read                       отметить все уведомления прочитанными
  trace on|off               показывать request-id каждого ответа: по нему путь запроса ищется в логах
  followers <ник> [после]    подписчики с именами, страницами по курсору
  posts <ник> [курсор]       посты автора страницами keyset
  following <ник>            подписки с именами
  friends <ник>              взаимные подписки
  comments <пост>            дерево комментариев
  reply <пост> <комм> <текст> ответ на комментарий
  suggest                    возможно, вы знакомы
  text <запрос>              полнотекстовый поиск постов с подсветкой
  people <ник>               поиск людей, опечатки прощаются
  settings [поле значение]   настройки: theme light|dark, language ru|en, private on|off, digest on|off
  search [--tag t] [--author ник] [--sort new|old|likes] [текст]
                             поиск постов
  export <файл>              все твои посты потоком в файл (NDJSON)
  pin|unpin <пост>           закрепить или открепить свой пост (не больше трёх)
  pinned <ник>               закреплённые посты
  like-comment <комм>        лайкнуть комментарий
  top-authors                авторы недели по лайкам
  stats [дней]               посты и лайки по дням (админ)
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
        cursor: Option<String>,
    },
    Chats,
    Typing(String),
    Online(String),
    Bell,
    ReadAll,
    Trace(bool),
    Related {
        method: &'static str,
        nick: String,
    },
    Daily(u64),
    Thread(String),
    Reply {
        post: String,
        parent: String,
        text: String,
    },
    Suggest,
    FullText(String),
    People(String),
    Settings(Option<(String, String)>),
    AuthorPosts(String, Option<String>),
    Search(Vec<(String, String)>, String),
    Export(String),
    Pin(String, bool),
    Pinned(String),
    LikeComment(String),
    TopAuthors,
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
            cursor: None,
        }),
        ("history", [with, limit]) => Ok(Command::History {
            with: with.to_string(),
            limit: number(limit)?,
            cursor: None,
        }),
        ("history", [with, limit, cursor]) => Ok(Command::History {
            with: with.to_string(),
            limit: number(limit)?,
            cursor: Some(cursor.to_string()),
        }),
        ("chats", []) => Ok(Command::Chats),
        ("typing", [nick]) => Ok(Command::Typing(nick.to_string())),
        ("online", [nick]) => Ok(Command::Online(nick.to_string())),
        ("bell", []) => Ok(Command::Bell),
        ("read", []) => Ok(Command::ReadAll),
        ("trace", ["on"]) => Ok(Command::Trace(true)),
        ("trace", ["off"]) => Ok(Command::Trace(false)),
        ("followers", [nick]) => Ok(Command::Related {
            method: "users.followers",
            nick: nick.to_string(),
        }),
        ("following", [nick]) => Ok(Command::Related {
            method: "users.following",
            nick: nick.to_string(),
        }),
        ("friends", [nick]) => Ok(Command::Related {
            method: "users.friends",
            nick: nick.to_string(),
        }),
        ("comments", [post]) => Ok(Command::Thread(post_id(post)?)),
        ("reply", [post, parent, _, ..]) => Ok(Command::Reply {
            post: post_id(post)?,
            parent: comment_id(parent)?,
            text: rest[post.len()..].trim()[parent.len()..].trim().to_string(),
        }),
        ("suggest", []) => Ok(Command::Suggest),
        ("text", [_, ..]) => Ok(Command::FullText(rest.to_string())),
        ("people", [nick]) => Ok(Command::People(nick.to_string())),
        ("settings", []) => Ok(Command::Settings(None)),
        ("settings", [field, value]) => Ok(Command::Settings(Some((
            field.to_string(),
            value.to_string(),
        )))),
        ("posts", [nick]) => Ok(Command::AuthorPosts(nick.to_string(), None)),
        ("posts", [nick, cursor]) => Ok(Command::AuthorPosts(
            nick.to_string(),
            Some(cursor.to_string()),
        )),
        ("followers", [nick, after]) => Ok(Command::Related {
            method: "users.followers",
            nick: format!("{nick} {after}"),
        }),
        ("search", [_, ..]) => search(rest),
        ("export", [file]) => Ok(Command::Export(file.to_string())),
        ("pin", [post]) => Ok(Command::Pin(post_id(post)?, true)),
        ("unpin", [post]) => Ok(Command::Pin(post_id(post)?, false)),
        ("pinned", [nick]) => Ok(Command::Pinned(nick.to_string())),
        ("like-comment", [comment]) => Ok(Command::LikeComment(comment_id(comment)?)),
        ("top-authors", []) => Ok(Command::TopAuthors),
        ("stats", []) => Ok(Command::Daily(7)),
        ("stats", [days]) => Ok(Command::Daily(number(days)?)),
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

fn comment_id(word: &str) -> Result<String, String> {
    let digits = word.strip_prefix("c_").unwrap_or(word);
    match digits.parse::<u64>() {
        Ok(number) => Ok(format!("c_{number}")),
        Err(_) => Err(format!(
            "«{word}» не id комментария, пиши число: reply 3 7 текст"
        )),
    }
}

fn search(rest: &str) -> Result<Command, String> {
    let mut filters = Vec::new();
    let mut words = Vec::new();
    let mut parts = rest.split_whitespace();
    while let Some(word) = parts.next() {
        match word.strip_prefix("--") {
            Some(flag @ ("tag" | "author" | "sort")) => {
                let value = parts
                    .next()
                    .ok_or_else(|| format!("после --{flag} нужно значение"))?;
                filters.push((flag.to_string(), value.to_string()));
            }
            Some(other) => {
                return Err(format!(
                    "нет фильтра --{other}: есть --tag, --author, --sort"
                ));
            }
            None => words.push(word),
        }
    }
    Ok(Command::Search(filters, words.join(" ")))
}
