# Хеш-таблицы

*Хеш-таблица* позволяет связывать значение с определённым ключом.
В других языках её называют [*unordered map* в C++](https://en.cppreference.com/w/cpp/container/unordered_map),
[*словарём* в Python](https://docs.python.org/3/tutorial/datastructures.html#dictionaries) или *ассоциативным массивом*.

Это та самая другая структура данных, про которую мы говорили раньше, когда
разбирали векторы.

## Кратко

`HashMap<K, V>` хранит пары «ключ, значение» без определённого порядка. Ключ должен реализовывать `Hash` и `Eq`.

```rust
use std::collections::HashMap;

let mut stock: HashMap<String, u32> = HashMap::new();
stock.insert("яблоко".to_string(), 3);
*stock.entry("яблоко".to_string()).or_insert(0) += 2;
*stock.entry("груша".to_string()).or_default() += 1;
println!("{:?}", stock.get("яблоко")); // Some(5)
```

`get` возвращает `Option`, `insert` перезаписывает старое значение. Метод `entry` удобен, когда нужно «вставить, если нет» или «обновить по старому значению»: он находит ключ один раз, а `or_insert` и `or_default` отдают изменяемую ссылку на значение.

## Что почитать ещё

- [Хранение ключей со связанными значениями в хеш-таблицах](https://doc.rust-lang.ru/book/ch08-03-hash-maps.html)
