# Свои features

Библиотека `logfmt` форматирует строки лога. Базовый формат нужен всем. JSON нужен многим, но не
всем. Цвет в терминале нужен единицам, и ради него не хочется тащить лишний крейт `colorize` в каждую
программу. Для такого и придуманы features: переключатели, которые включают куски кода и опциональные
зависимости.

## Что сделать

1. В `Cargo.toml` объяви `[features]`:
   - `json`: без зависимостей, включена по умолчанию;
   - `color`: включает опциональную зависимость `colorize` записью `dep:colorize`.
2. В `src/lib.rs` пометь `#[cfg(feature = "...")]` код, который существует только при своей feature.
   Без этого библиотека не соберётся без `color`: `colorize` тогда не подключён вообще.

## Приёмка

Тесты запускают `examples/probe.rs` с разными флагами и смотрят, какие features реально включились:

```
cargo run --example probe                              # json
cargo run --example probe --no-default-features        # пусто
cargo run --example probe --features color             # json,color
cargo run --example probe --no-default-features --features color   # color
cargo tree --no-default-features                       # colorize здесь быть не должно
```

Ещё проверяется, что у пакета нет публичной feature с именем `colorize`: запись `dep:` её прячет.
