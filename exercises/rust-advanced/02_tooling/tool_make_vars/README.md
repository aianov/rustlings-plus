# Параметры в Makefile

В проекте два сервиса, `api` и `auth`. Запускать хочется любой из них, не правя файл, а ещё иногда в
release-режиме. Пока `Makefile` умеет запускать только `api`, да и тот зашит в рецепт.

## Что должно получиться

```
make run                      запускает api
make run SERVICE=auth         запускает auth
SERVICE=auth make run         тоже auth
make run SERVICE=api          при SERVICE=auth в окружении запускает всё равно api
make run PROFILE=release      добавляет --release к cargo
make build PROFILE=release    то же самое для сборки
```

Если сервиса нет, `make` завершается ошибкой (это обеспечивает cargo). Цели `build` и `run` работают, даже
если в папке есть файлы или каталоги с такими именами.

## Приёмка

```
cargo test --manifest-path acceptance/Cargo.toml
```

Тесты запускают `make` с аргументами, с переменными окружения и с флагом `-n`, который печатает
команды, ничего не запуская. Так видно, какие флаги получит cargo.
