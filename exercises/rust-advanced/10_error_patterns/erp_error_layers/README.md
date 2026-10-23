# Проект: ошибки в трёх слоях

Бухгалтерия соцсети хранит остатки внутренней валюты «лайков» в текстовом журнале. Три крейта: `ledger-store`
разбирает журнал, `ledger-core` переводит деньги по правилам, `ledger-cli` принимает команды и сообщает о
результате. Когда что-то идёт не так, пользователь должен услышать одну ясную строку, а программа должна уметь
выбрать код возврата по настоящей причине, не разбирая текст.

## Что нужно сделать

В каждом крейте объявлены типы и сигнатуры, тела пустые. Правило одно: у каждого крейта своя ошибка и свой алиас
`Result`, причины хранятся в полях и отдаются через `source`, `Display` описывает только свой уровень.

### ledger-store

1. `Store::parse(text)`: строки вида `имя=сумма`, пустые строки пропускаются (но считаются при нумерации, с единицы).
   Нет `=`: `MissingSeparator`. Сумма не число: `BadLine` с номером, всей строкой (без пробелов по краям) и `ParseIntError`
   в `source`. Повторное имя: `Duplicate` с номером второй строки.
2. `Store::get(name)` и `Store::set(name, amount)`: неизвестное имя это `NoSuchAccount`.
3. Тексты: `missing '=' on line N`, `bad line N: "<строка>"` (строка через `{:?}`), `duplicate account X on line N`,
   `account X does not exist`.

### ledger-core

4. `Bank::open(text)`: ошибка хранилища оборачивается в `Open { source }` с текстом `cannot open ledger`.
5. `Bank::balance(name)`: ошибка хранилища оборачивается в `Lookup { source }` с текстом `lookup failed`.
6. `Bank::transfer(from, to, amount)`: сумма должна быть положительной (`InvalidAmount(n)`, текст
   `amount must be positive, got N`), оба счёта должны существовать (`Lookup`), денег должно хватать
   (`Insufficient { account, needed, available }`, текст `<account> has <available>, needs <needed>`).
   При любой ошибке ничего не меняется. Перевод самому себе остатка не меняет.

### ledger-cli

7. `run(ledger, command)` понимает `balance <имя>` (ответ `имя: сумма`) и `transfer <от> <кому> <сумма>`
   (ответ `transferred N from A to B`). Команду проверяют до открытия журнала. Порядок проверок: команда,
   потом сумма, потом открытие журнала. Возвращает `Result<String, Box<dyn Error + Send + Sync>>`.
8. Свой `Error` крейта: `Usage(String)` хранит введённую команду, а `Display` у него всегда `usage: balance <name> | transfer <from> <to> <amount>`,
   `BadAmount { text, source }` (`invalid amount "<текст>"`), `Core(ledger_core::Error)` (`command failed`).
9. `report(err)` собирает всю цепочку причин через `: `. `exit_code(err)` обходит цепочку: `Insufficient` это 2,
   `NoSuchAccount` это 3, `Usage` это 64, всё остальное 1. Порядок проверки идёт от внешней ошибки к корню.

## Как проверить

```
cargo test --workspace
```

Приёмочные тесты лежат в `crates/*/tests/acceptance.rs`, их менять нельзя.

## Подсказки по теории

README главы, разделы про `source`, `From` на границе крейтов и обход цепочки. Если `report` печатает одно и то же
дважды, значит, чей-то `Display` пересказывает причину.
