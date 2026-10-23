// Это квиз по следующим разделам:
// - Строки
// - Векторы
// - Семантика перемещения
// - Модули
// - Перечисления
//
// Давай построим небольшую машину в виде функции. На вход мы подадим
// список строк и команд. Эти команды определяют, какое действие
// будет применено к строке. Это может быть:
// - перевод строки в верхний регистр
// - обрезка пробелов по краям строки
// - добавление "bar" к строке заданное число раз
//
// Точный вид будет таким:
// - Вход это вектор кортежей из двух элементов,
//   первый элемент это строка, второй это команда.
// - Выход это вектор строк.

enum Command {
    Uppercase,
    Trim,
    Append(usize),
}

mod my_module {
    use super::Command;

    // TODO: Дополни функцию, как описано выше.
    // pub fn transformer(input: ???) -> ??? { ??? }
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    // TODO: Что нужно импортировать, чтобы `transformer` был в области видимости?
    // use ???;
    use super::Command;

    #[test]
    fn it_works() {
        let input = vec![
            ("hello".to_string(), Command::Uppercase),
            (" all roads lead to rome! ".to_string(), Command::Trim),
            ("foo".to_string(), Command::Append(1)),
            ("bar".to_string(), Command::Append(5)),
        ];
        let output = transformer(input);

        assert_eq!(
            output,
            [
                "HELLO",
                "all roads lead to rome!",
                "foobar",
                "barbarbarbarbarbar",
            ]
        );
    }
}
