# Из простыни в дерево

Ядро магазина `shopcore` написано одним файлом `src/lib.rs`: каталог товаров и склад, счёт с налогом,
клиенты и бонусы. Файл разросся, в нём тяжело ориентироваться, а пользователи крейта вынуждены писать
пути вроде `shopcore::billing::invoice::Invoice`, которые завтра придётся менять.

## Что должно получиться

Файлы и папки:

```
crates/shopcore/src/
  lib.rs
  billing/    mod.rs  invoice.rs  money.rs  tax.rs
  catalog/    mod.rs  product.rs  stock.rs
  customers/  mod.rs  account.rs  loyalty.rs
```

1. Поведение не меняется: тесты в `crates/shopcore/tests/api.rs` проходят.
2. Раскладка единая: модуль, у которого есть вложенные, это папка с `mod.rs`; рядом с такой папкой
   нет одноимённого `.rs` файла.
3. `lib.rs` содержит только `mod` и `pub use`, ни одной функции и структуры. Ни один файл не длиннее 50 строк.
4. Модули `billing`, `catalog` и `customers` приватные. Наружу выходит плоский фасад:
   `Product`, `Stock`, `StockError`, `Invoice`, `Account`, `format_cents`, `add_vat`, `points_for`,
   `welcome_points`. Пути вида `shopcore::billing::invoice::Invoice` снаружи не работают.

## Приёмка

```
cargo test --manifest-path acceptance/Cargo.toml
```
