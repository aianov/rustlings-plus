# npb_build: social-proto

Проверочная задача главы. Сервисы соцсети будут обмениваться постами, и описывать их структуры руками в каждом сервисе значит
рано или поздно разойтись в номерах полей. Нужен один источник правды: файлы `.proto`, из которых при сборке генерируется
Rust-код. Этим занимается `prost-build`, а запускает его скрипт сборки `build.rs`.

## Что уже есть

`Cargo.toml` с зависимостями `prost`, `prost-types` и `prost-build` (в `[build-dependencies]`, то есть только для сборки) и
приёмочные тесты. Всё остальное создаёшь ты. Для работы нужен установленный `protoc` (проверь `protoc --version`; на macOS это
`brew install protobuf`). Если он лежит не в `PATH`, укажи его в переменной окружения `PROTOC`.

## Что сделать

1. `proto/social/v1/common.proto`: пакет `social.v1`, сообщение `Author { uint64 id = 1; string handle = 2; }`.
2. `proto/social/v1/post.proto`: тот же пакет, импорт `common.proto` и `google/protobuf/timestamp.proto`, перечисление `Visibility`
   и сообщение `Post`:

   | поле | номер | тип |
   |------|-------|-----|
   | id | 1 | uint64 |
   | author | 2 | Author |
   | text | 3 | string |
   | created_at | 4 | google.protobuf.Timestamp |
   | hashtags | 5 | repeated string |
   | visibility | 6 | Visibility |

   Значения `Visibility`: `VISIBILITY_UNSPECIFIED = 0`, `VISIBILITY_PUBLIC = 1`, `VISIBILITY_FOLLOWERS = 2` (по правилам
   стиля protobuf нулевое значение называется `..._UNSPECIFIED`, а имена значений начинаются с имени перечисления).
3. `build.rs`: вызывает `prost_build::compile_protos` для обоих файлов, корнем для импортов служит папка `proto/`. Скрипт должен
   сообщать cargo, когда его нужно запускать заново (`cargo:rerun-if-changed`).
4. `src/lib.rs`: подключает сгенерированный код. Prost кладёт его в `OUT_DIR` в файл с именем пакета (`social.v1.rs`), подключается он
   макросом `include!` с путём из `env!("OUT_DIR")`. Модули `social::v1` повторяют имя пакета: `social_proto::social::v1::Post`.

## Критерии приёмки

`cargo test`:

- `Post` кодируется ровно в согласованные байты и читается из них (номера и типы полей);
- `created_at` это настоящий `prost_types::Timestamp`, а не копия, сгенерированная заново;
- у перечисления чистые Rust-имена (`Visibility::Followers`), а неизвестное число не превращается в вариант;
- файлы лежат по пути, повторяющему пакет и версию, импорты идут от корня include-пути;
- `build.rs` перезапускается при изменении схем.

## Структура

```
npb_build/
  Cargo.toml                      готово
  build.rs                        твой
  proto/social/v1/common.proto    твой
  proto/social/v1/post.proto      твой
  src/lib.rs                      твой
  tests/acceptance.rs
```

## Подсказки по теории

Пути в `import` считаются от корня include-пути, который ты передаёшь в `compile_protos` вторым аргументом, а не от файла,
в котором написан импорт. Папку `proto/social/v1/` делают по схеме «пакет, версия»: когда появится несовместимая версия, она
получит пакет `social.v2` рядом со старой, и сервисы смогут переезжать по очереди.
