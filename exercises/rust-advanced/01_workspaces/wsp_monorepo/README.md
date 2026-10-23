# Квиз: монорепозиторий

Из нескольких готовых крейтов нужно собрать настоящий монорепозиторий. Исходники уже лежат на местах,
а манифестов нет совсем: ни корневого, ни у членов. Вся работа на тебе: объединить, раздать общее,
навести порядок в публичных границах.

## Дерево

```
.
  Cargo.toml                          корень (напиши)
  src/crates/
    contracts/      UserId, Message, JSON; единственная внешняя зависимость serde_json
    text-engine/    tokenize, mentions
    store-engine/   Store: сообщения и индекс по автору
  src/microservices/
    greeter-service/    cargo run -p greeter-service -- "hi @ann and @bob"
    archive-service/    читает со stdin строки `id: текст`, печатает JSON-сводку
  acceptance/       приёмка
```

## Требования

1. Воркспейс виртуальный, resolver 3, члены заданы двумя масками: `src/crates/*` и
   `src/microservices/*`. Пять пакетов: `contracts`, `text-engine`, `store-engine`, `greeter-service`,
   `archive-service`.
2. `[workspace.package]`: version 0.1.0, edition 2024, license MIT, rust-version 1.88. Все пять
   наследуют все четыре поля.
3. Все зависимости объявлены в `[workspace.dependencies]` (`serde_json` и три внутренних крейта) и
   подключены в членах записью `workspace = true`.
4. Стрелки: `contracts` зависит только от `serde_json`; движки только от `contracts`; сервисы от
   нужных им крейтов; сервисы друг от друга не зависят.
5. `[workspace.lints]`: `unsafe_code = "forbid"` и `clippy::unwrap_used = "deny"`; каждый член
   подключает правила через `[lints] workspace = true`. Код при этом чист по clippy.
6. Три библиотеки отдают наружу плоский фасад, а внутренние модули скрыты:
   - `contracts`: `UserId`, `Message`;
   - `text-engine`: функции `tokenize(text)` и `mentions(text)` (имена упоминаний без `@`). Наружу
     имена именно такие, а как называются функции внутри модулей, решаешь ты;
   - `store-engine`: `Store`.
   Пути вида `text_engine::lexer::split_words` снаружи не работают. Это потребует поправить
   `use` в соседних файлах.
7. Раскладка модулей единая: у каждого модуля с вложенными папка с `mod.rs`.
8. `cargo run -p greeter-service -- "hi @ann and @bob"` печатает `greeting: ann, bob`;
   `archive-service` на вводе `1: hello`, `2: hi`, `1: again` и строке без двоеточия печатает
   JSON с `stored` 3 и `authors` 2.
9. Один Cargo.lock, один target; `serde_json` в единственной версии.

## Приёмка

```
cargo test --manifest-path acceptance/Cargo.toml
cargo clippy --workspace --all-targets -- -D warnings
```

Подсказка про порядок: корень, потом `contracts`, потом движки, потом сервисы. После каждого шага
`cargo build --workspace`.
