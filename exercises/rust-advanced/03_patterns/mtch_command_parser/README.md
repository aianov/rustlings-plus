# Квиз: команды чата и сессия

Мини-соцсеть получает личные сообщения. Пользователь печатает строки в терминале, и по строке
надо понять, что он имел в виду: `/msg ann привет`, `/history ann 20`, просто текст. Чуть выше
лежит второй слой: что вообще разрешено делать, зависит от того, кто ты сейчас. Анонимный
пользователь не пишет, вошедший без собеседника не может «говорить в пустоту», после `/quit`
молчат все команды. Это два небольших механизма, и оба держатся на образцах: разбор строки на
срезах слов и автомат сессии на `match`.

## Что уже есть

Пакет `chat-session` с типами в `src/command.rs` и `src/session.rs` и пустыми функциями `parse` и
`Session::step`. Типы менять не нужно, приёмка использует их как написаны. Тесты лежат в
`tests/acceptance.rs`.

## Что сделать

### 1. `command::parse(line) -> Result<Command, ParseError>`

1. Края строки обрезаются. Пустая строка это `ParseError::Empty`.
2. Строка без ведущего `/` это `Command::Say(текст)`, внутренние пробелы остаются как есть.
   Строка, начинающаяся с `//`, тоже `Say`, но без первого слэша (так в чате пишут символ `/`).
3. Остальное команды. Слова разделены любым числом пробелов.

   | строка | результат |
   |---|---|
   | `/login <ник>` | `Login(ник)` |
   | `/logout`, `/whoami`, `/quit` | `Logout`, `Whoami`, `Quit` |
   | `/msg <ник> <слово>...` | `Msg { to, text }`, слова текста склеены одним пробелом |
   | `/history <ник> [число]` | `History { with, limit }`, число от 1 до 100 |

4. Ник это от 3 до 16 символов из `a-z`, `0-9`, `_`. Иначе `InvalidNick(ник как введён)`.
5. Ошибки идут в таком порядке: неизвестная команда (`Unknown(имя)`, для одинокого `/` и для
   `/` с пробелом сразу после него имя пустое), потом форма аргументов (`MissingArg { command, what }` или `TooManyArgs(команда)`),
   потом содержимое (`InvalidNick`, потом `BadNumber(как введено)`, потом `LimitOutOfRange(n)`).
   Имена команд чувствительны к регистру.
6. Паниковать на любом вводе нельзя. Ветки `_ =>` в файле нет, а разбор слов построен на
   срезовых образцах и хотя бы один ранний выход написан через let-else.

### 2. `Session::step(self, command) -> Result<(Session, Reply), Rejected>`

Состояния: `Anonymous`, `LoggedIn { user }`, `Chatting { user, peer }`, `Closed`. Переход берёт
сессию по значению и либо возвращает новую и ответ, либо `Rejected` с причиной и исходной сессией
(её нельзя терять: вызывающий хочет продолжить с ней).

| состояние | команда | результат |
|---|---|---|
| Anonymous | `Login(u)` | `LoggedIn{u}`, `Welcome(u)` |
| Anonymous | `Whoami` | то же состояние, `You(None)` |
| Anonymous | `Quit` | `Closed`, `Bye` |
| Anonymous | `Say`, `Logout`, `Msg`, `History` | отказ `NotLoggedIn` |
| LoggedIn | `Login` | отказ `AlreadyLoggedIn` |
| LoggedIn | `Logout` | `Anonymous`, `Goodbye` |
| LoggedIn | `Whoami` | то же, `You(Some(u))` |
| LoggedIn | `Msg{to,text}` | `Chatting{u, peer: to}`, `Sent{to,text}`; себе: отказ `SelfMessage` |
| LoggedIn | `History{with,limit}` | то же, `Show{with,limit}` |
| LoggedIn | `Say` | отказ `NoPeer` |
| LoggedIn | `Quit` | `Closed`, `Bye` |
| Chatting | `Say(text)` | то же, `Sent{to: peer, text}` |
| Chatting | `Msg{to,text}` | диалог переключается на `to`, `Sent`; себе: отказ `SelfMessage` |
| Chatting | `Login` | отказ `AlreadyLoggedIn` |
| Chatting | `Logout`, `Quit`, `Whoami`, `History` | как у LoggedIn |
| Closed | что угодно | отказ `SessionClosed` |

В `session.rs` тоже нет `_ =>`: каждое состояние и каждая команда названы. Если завтра в `Command`
появится вариант, компилятор покажет все места, которые надо дописать.

## Как проверить

```
cargo test
cargo clippy --all-targets -- -D warnings
```

## Подсказки по теории

Срезовые образцы и let-else разобраны в README главы, автомат на `(состояние, событие)` тоже. Для
`Msg` текст это «всё после ника»: образец `["msg", nick, text @ ..]`. Для лимита истории пригодится
`let limit @ 1..=100 = limit else { ... }`. В `step` удобнее сначала `match` по состоянию, а внутри
по команде: так видно, что ни одно сочетание не забыто.
