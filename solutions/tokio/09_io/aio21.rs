use std::io;
use std::path::Path;

use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

async fn append_line(path: &Path, line: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).await?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path).await?;
    file.write_all(format!("{line}\n").as_bytes()).await?;
    file.flush().await
}

async fn read_lines(path: &Path) -> io::Result<Vec<String>> {
    let file = match fs::File::open(path).await {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut lines = BufReader::new(file).lines();
    let mut out = Vec::new();
    while let Some(line) = lines.next_line().await? {
        out.push(line);
    }
    Ok(out)
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
