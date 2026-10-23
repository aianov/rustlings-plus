// Деплой-бот принимает команды из чата, уже разбитые на слова:
//   deploy <окружение> [сервис...]    rollback <окружение> <версия>
//   scale <сервис> <число>            status
// Пока разбор написан бы как `words[0]`, `words.len() == 3` и `words[2]`, а любая лишняя проверка
// длины превращается в потенциальную панику. Срезовые образцы описывают форму команды целиком.
// TODO: Реализуй `parse` одним `match` по срезу слов, а ниже две мелкие функции на срезах.
// Ошибки: неизвестная команда, неправильное число аргументов, не число там, где нужно число.

#[derive(Debug, PartialEq)]
pub enum Cmd<'a> {
    Empty,
    Deploy { env: &'a str, services: &'a [&'a str] },
    Rollback { env: &'a str, version: u32 },
    Scale { service: &'a str, replicas: u8 },
    Status,
}

#[derive(Debug, PartialEq)]
pub enum Error<'a> {
    Unknown(&'a str),
    WrongArgs(&'a str),
    BadNumber(&'a str),
}

pub fn parse<'a>(words: &'a [&'a str]) -> Result<Cmd<'a>, Error<'a>> {
    todo!("разобрать слова в команду")
}

pub fn ends(items: &[i32]) -> Option<(i32, i32)> {
    todo!("первый и последний элементы; у одного элемента они совпадают; у пустого среза их нет")
}

pub fn inner<T>(items: &[T]) -> &[T] {
    todo!("всё, кроме первого и последнего; у коротких срезов (меньше двух элементов) пусто")
}

fn main() {
    println!("{:?}", parse(&["status"]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        assert_eq!(parse(&[]), Ok(Cmd::Empty));
    }

    #[test]
    fn deploy_takes_env_and_any_number_of_services() {
        assert_eq!(parse(&["deploy", "prod"]), Ok(Cmd::Deploy { env: "prod", services: &[] }), "без сервисов значит все");
        assert_eq!(parse(&["deploy", "stage", "api"]), Ok(Cmd::Deploy { env: "stage", services: &["api"] }));
        assert_eq!(
            parse(&["deploy", "prod", "api", "auth", "chat"]),
            Ok(Cmd::Deploy { env: "prod", services: &["api", "auth", "chat"] })
        );
        assert_eq!(parse(&["deploy"]), Err(Error::WrongArgs("deploy")), "окружение обязательно");
    }

    #[test]
    fn rollback_and_scale_take_exact_arguments() {
        assert_eq!(parse(&["rollback", "prod", "42"]), Ok(Cmd::Rollback { env: "prod", version: 42 }));
        assert_eq!(parse(&["rollback", "prod"]), Err(Error::WrongArgs("rollback")));
        assert_eq!(parse(&["rollback", "prod", "42", "now"]), Err(Error::WrongArgs("rollback")), "лишний аргумент это ошибка");
        assert_eq!(parse(&["scale", "api", "3"]), Ok(Cmd::Scale { service: "api", replicas: 3 }));
        assert_eq!(parse(&["scale", "api"]), Err(Error::WrongArgs("scale")));
    }

    #[test]
    fn numbers_are_checked() {
        assert_eq!(parse(&["rollback", "prod", "v2"]), Err(Error::BadNumber("v2")));
        assert_eq!(parse(&["rollback", "prod", "-1"]), Err(Error::BadNumber("-1")));
        assert_eq!(parse(&["scale", "api", "300"]), Err(Error::BadNumber("300")), "в u8 триста не влезает");
        assert_eq!(parse(&["scale", "api", "many"]), Err(Error::BadNumber("many")));
    }

    #[test]
    fn status_and_unknown() {
        assert_eq!(parse(&["status"]), Ok(Cmd::Status));
        assert_eq!(parse(&["status", "all"]), Err(Error::WrongArgs("status")), "у status аргументов нет");
        assert_eq!(parse(&["reboot"]), Err(Error::Unknown("reboot")));
        assert_eq!(parse(&["reboot", "now", "please"]), Err(Error::Unknown("reboot")), "название неизвестной команды, а не аргумента");
    }

    #[test]
    fn wrong_args_wins_over_bad_number() {
        assert_eq!(parse(&["scale", "x", "y", "z"]), Err(Error::WrongArgs("scale")), "сначала форма, потом содержимое");
    }

    #[test]
    fn written_with_slice_patterns() {
        let prod = include_str!("mtch5.rs").split("#[cfg(test)]").next().expect("файл не пуст");
        assert!(prod.contains("@ .."), "разбор должен идти срезовыми образцами с хвостом `@ ..`");
        assert!(!prod.contains("words["), "индексы вида words[0] не нужны: форма команды описывается образцом");
    }

    #[test]
    fn ends_of_slices() {
        assert_eq!(ends(&[]), None);
        assert_eq!(ends(&[7]), Some((7, 7)));
        assert_eq!(ends(&[1, 2]), Some((1, 2)));
        assert_eq!(ends(&[1, 2, 3, 4]), Some((1, 4)));
    }

    #[test]
    fn inner_of_slices() {
        assert_eq!(inner::<i32>(&[]), &[] as &[i32]);
        assert_eq!(inner(&[1]), &[] as &[i32], "у одного элемента серединки нет");
        assert_eq!(inner(&[1, 2]), &[] as &[i32]);
        assert_eq!(inner(&[1, 2, 3]), &[2]);
        assert_eq!(inner(&["a", "b", "c", "d"]), &["b", "c"]);
    }
}
