// Клиент общается с сервером тем же протоколом: команда это массив, а каждое слово команды
// отдельная bulk string, даже если это число. Кодировщик bulk уже готов (он из прошлой задачи).
// TODO: Напиши `array` (собирает массив из уже закодированных элементов), `null_array`
// (отсутствующий массив, не путай с пустым) и `command` (слова команды как массив строк).

fn bulk(value: Option<&[u8]>) -> Vec<u8> {
    match value {
        None => b"$-1\r\n".to_vec(),
        Some(bytes) => {
            let mut out = format!("${}\r\n", bytes.len()).into_bytes();
            out.extend_from_slice(bytes);
            out.extend_from_slice(b"\r\n");
            out
        }
    }
}

fn array(items: &[Vec<u8>]) -> Vec<u8> {
    todo!()
}

fn null_array() -> Vec<u8> {
    todo!()
}

fn command(words: &[&[u8]]) -> Vec<u8> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_of_ready_items() {
        let items = vec![b":1\r\n".to_vec(), b"+OK\r\n".to_vec(), bulk(Some(b"ab"))];
        assert_eq!(array(&items), b"*3\r\n:1\r\n+OK\r\n$2\r\nab\r\n".to_vec(), "заголовок с числом элементов, потом элементы как есть");
    }

    #[test]
    fn empty_and_missing_array_differ() {
        assert_eq!(array(&[]), b"*0\r\n".to_vec(), "пустой массив: ноль элементов");
        assert_eq!(null_array(), b"*-1\r\n".to_vec(), "отсутствующий массив: минус один");
    }

    #[test]
    fn set_command_is_array_of_bulks() {
        assert_eq!(
            command(&[b"SET", b"user:1", b"10"]),
            b"*3\r\n$3\r\nSET\r\n$6\r\nuser:1\r\n$2\r\n10\r\n".to_vec(),
            "число 10 тоже идёт bulk-строкой, а не целым: у команд все аргументы строки"
        );
    }

    #[test]
    fn command_argument_may_contain_crlf() {
        assert_eq!(
            command(&[b"SET", b"k", b"a\r\nb"]),
            b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$4\r\na\r\nb\r\n".to_vec(),
            "значение с переводом строки внутри уезжает без изменений, длина в заголовке 4"
        );
    }

    #[test]
    fn command_without_words_is_empty_array() {
        assert_eq!(command(&[]), b"*0\r\n".to_vec(), "ни одного слова: пустой массив");
    }
}
