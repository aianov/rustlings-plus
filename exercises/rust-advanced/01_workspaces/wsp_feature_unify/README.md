# Одна зависимость, разные результаты

В воркспейсе библиотека `greeter` и два приложения: `quiet-app` здоровается вежливо, `loud-app`
кричит. Тесты каждого приложения по отдельности зелёные (`cargo test -p quiet-app`), а общий
`cargo test --workspace` падает. Утром в CI это выглядит как проклятие: вчера всё проходило.

## Что должно получиться

1. `cargo test -p quiet-app`, `cargo test -p loud-app`, `cargo test -p greeter --all-features` и
   `cargo test --workspace` проходят все четыре.
2. Приложения сохраняют своё поведение: `quiet_app::welcome("Ann")` даёт `Hello, Ann!`,
   `loud_app::announce("Ann")` даёт `HELLO, ANN!`.
3. `greeter::greet` не меняется от features вообще. `greeter::shout` доступна всегда.

Тесты внутри крейтов (`crates/*/tests/`) править нельзя.

## Приёмка

```
cargo test --manifest-path acceptance/Cargo.toml
```
