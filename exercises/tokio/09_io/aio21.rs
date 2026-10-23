// Журнал событий живёт в файле. Нужны два помощника: дописать строку в конец файла (создав
// и файл, и недостающие папки по пути) и прочитать все строки обратно. Если файла ещё нет,
// это не авария, а просто пустой журнал. Всё через `tokio::fs`, потому что блокирующий
// `std::fs` в async-задаче держит поток рантайма.
// TODO: Напиши `append_line` и `read_lines`.
use std::io;
use std::path::Path;

async fn append_line(path: &Path, line: &str) -> io::Result<()> {
    let _ = (path, line);
    todo!()
}

async fn read_lines(path: &Path) -> io::Result<Vec<String>> {
    let _ = path;
    todo!()
}

#[tokio::main]
async fn main() {
    let path = std::env::temp_dir().join("aio21-demo").join("log.txt");
    append_line(&path, "first").await.unwrap();
    println!("{:?}", read_lines(&path).await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("aio21-{}-{name}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        dir
    }

    #[tokio::test]
    async fn creates_folders_and_appends() {
        let dir = scratch_dir("append").await;
        let path = dir.join("deep").join("er").join("journal.log");
        append_line(&path, "one").await.unwrap();
        append_line(&path, "two").await.unwrap();
        let text = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(text, "one\ntwo\n", "вторая запись должна дописываться в конец, а не затирать первую");
        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn reads_back() {
        let dir = scratch_dir("read").await;
        let path = dir.join("journal.log");
        for line in ["a", "", "c"] {
            append_line(&path, line).await.unwrap();
        }
        assert_eq!(read_lines(&path).await.unwrap(), vec!["a", "", "c"], "читаем строки в том порядке, в каком писали");
        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn missing_file_is_empty_journal() {
        let dir = scratch_dir("missing").await;
        let got = read_lines(&dir.join("nothing.log")).await;
        assert_eq!(got.unwrap(), Vec::<String>::new(), "файла нет: это пустой журнал, а не ошибка");
    }
}
