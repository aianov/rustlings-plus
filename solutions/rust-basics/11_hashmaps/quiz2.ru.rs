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

    // Решение с циклом. Версию с итераторами смотри в
    // `transformer_iter`.
    pub fn transformer(input: Vec<(String, Command)>) -> Vec<String> {
        let mut output = Vec::new();

        for (string, command) in input {
            // Создаём новую строку.
            let new_string = match command {
                Command::Uppercase => string.to_uppercase(),
                Command::Trim => string.trim().to_string(),
                Command::Append(n) => string + &"bar".repeat(n),
            };

            // Кладём новую строку в выходной вектор.
            output.push(new_string);
        }

        output
    }

    // То же, что `transform`, но для сравнения использует итератор вместо цикла.
    // Не переживай, итераторы мы ещё потренируем ;)
    pub fn transformer_iter(input: Vec<(String, Command)>) -> Vec<String> {
        input
            .into_iter()
            .map(|(string, command)| match command {
                Command::Uppercase => string.to_uppercase(),
                Command::Trim => string.trim().to_string(),
                Command::Append(n) => string + &"bar".repeat(n),
            })
            .collect()
    }
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    // Импортируем `transformer`.
    use super::my_module::transformer;

    use super::Command;
    use super::my_module::transformer_iter;

    #[test]
    fn it_works() {
        for transformer in [transformer, transformer_iter] {
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
}
