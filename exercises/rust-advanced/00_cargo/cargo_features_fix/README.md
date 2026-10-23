# Починка features

Пакет `cachelib` собирается с настройками по умолчанию, и все довольны. Но у него две болячки.

1. Включишь `--features stats` и получишь `unresolved import tinymap`. Крейт `tinymap` объявлен
   опциональным, а feature, которая его включает, об этом не знает.
2. Есть feature `no_log`. Включаешь её, и логирование пропадает. Звучит разумно, пока в одной
   программе два крейта не потребуют разное: cargo собирает зависимость один раз с объединением всех
   запрошенных features, и `no_log` от одного крейта отключит логи у всех. Feature должна только
   добавлять возможности, ничего не отнимая.

## Что сделать

Почини `Cargo.toml` и `src/lib.rs`:

- `stats` включает `tinymap` через `dep:`;
- вместо `no_log` заведи положительную feature `log`, включи её по умолчанию и повесь на неё код;
- выключается логирование флагом `--no-default-features`, а не специальной feature.

## Приёмка

```
cargo run --example probe                          # get,put,log
cargo run --example probe --no-default-features    # get,put
cargo run --example probe --features stats         # get,put,log,stats
cargo run --example probe --all-features           # включено всё, и ничего не пропало
```
