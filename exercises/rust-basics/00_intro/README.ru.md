# Введение

Чтобы вывести текст в консоль, Rust использует макросы `print!` и `println!`.

## Кратко

`println!("Привет, {}!", name)` печатает строку и переводит строку. `print!` делает то же самое, но без перевода строки в конце. Фигурные скобки `{}` это места, куда подставляются аргументы. Восклицательный знак в конце имени говорит, что это макрос, а не обычная функция: поэтому `printline!` не существует, а `println!` существует.

```rust
fn main() {
    let name = "мир";
    println!("Привет, {name}!");
}
```

## Что почитать ещё

- [Hello World](https://doc.rust-lang.org/rust-by-example/hello.html)
- [Formatted print](https://doc.rust-lang.org/rust-by-example/hello/print.html)
