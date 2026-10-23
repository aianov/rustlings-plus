# Квиз: пакет-утилита textstat

Собираем всё, что было в главе, в одном маленьком пакете. `textstat` читает текст из stdin и печатает
статистику: строки, слова, символы или самые частые слова без стоп-слов. Заготовка лежит перед тобой:
в `Cargo.toml` только имя и версия, в `src/lib.rs` сигнатуры с `todo!()`, рядом два готовых вспомогательных
крейта в `crates/` и словарь стоп-слов в `data/stopwords.txt`.

## Что должно получиться

```
textstat/
  Cargo.toml
  build.rs
  data/stopwords.txt
  crates/tokenizer/        готов: words(text) режет текст на слова в нижнем регистре
  crates/samples/          готов: TEXT, пример для тестов
  src/lib.rs
  src/main.rs              бинарник textstat
  src/bin/words.rs         бинарник textstat-words
  tests/acceptance.rs      приёмка (править нельзя)
```

## Требования

1. `Cargo.toml`: edition 2024, `rust-version = "1.88"`. Второй бинарник называется `textstat-words`
   и собирается из `src/bin/words.rs`.
2. Зависимости: `tokenizer` нужен коду программы, `samples` нужен только тестам. Build-зависимостей нет.
3. Feature `json`, выключенная по умолчанию, добавляет `textstat::render_json` и флаг `--json`
   (`{"lines":2,"words":10,"chars":41}`). Без feature флаг `--json` печатает в stderr объяснение со
   словом json и завершает программу с кодом 2.
4. `build.rs` читает `data/stopwords.txt` (комментарии с `#`, пустые строки и повторы игнорируются),
   пишет в `OUT_DIR` отсортированный массив стоп-слов и передаёт в код строку
   `textstat <версия> (<N> stopwords)` как `TEXTSTAT_VERSION`. Правка словаря обязана попадать в
   следующую сборку, в `src/` скрипт ничего не пишет.
5. Библиотека: `analyze`, `top_words` (по убыванию частоты, при равенстве по алфавиту, без стоп-слов),
   `render_text`, `stopwords`, константа `VERSION`.
6. Бинарник `textstat`: без флагов печатает `render_text`, `--top N` печатает `слово число` по строке,
   `--version` печатает `VERSION`. Разбор аргументов делай руками, готовых парсеров в курсе пока нет.
7. `[lints]`: `unsafe_code = "forbid"`, `clippy::unwrap_used = "deny"`.
8. `[profile.release]`: `lto = true`, `codegen-units = 1`.

## Приёмка

```
cargo test
cargo clippy --all-targets -- -D warnings
echo "The cat and the dog" | cargo run -- --top 2
```

## Про выбор библиотек

Здесь ни одной внешней зависимости нет: крейты лежат внутри проекта. В реальной работе вместо
`tokenizer` ты мог бы взять готовый, и вопросы главы про выбор библиотеки (лицензия, свежесть,
unsafe, размер дерева) нужны именно для этого момента.
