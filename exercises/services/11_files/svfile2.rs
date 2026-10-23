// Файл сохраняется на диск, и в любой момент читатель должен видеть либо старое содержимое целиком, либо новое целиком.
// Запись прямо в итоговое имя этого не даёт, а питание иногда выключают. Цель: `write_atomic` кладёт данные так,
// чтобы по итоговому пути никогда не лежало ничего недописанного, чтобы данные успели дойти до диска раньше, чем новое
// имя станет видимым, и чтобы при любой ошибке итоговый файл остался нетронутым, а рядом не осталось мусора.
// Диск здесь абстрактный (`Disk`), чтобы тесты видели порядок операций.

use std::io;
use std::path::{Path, PathBuf};

fn main() {}

pub trait Disk {
    fn create(&mut self, path: &Path) -> io::Result<()>;
    fn append(&mut self, path: &Path, data: &[u8]) -> io::Result<()>;
    fn sync(&mut self, path: &Path) -> io::Result<()>;
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove(&mut self, path: &Path) -> io::Result<()>;
}

pub fn write_atomic(disk: &mut impl Disk, dest: &Path, chunks: &[&[u8]], nonce: u64) -> io::Result<()> {
    let _ = (&disk, dest, chunks, nonce);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeDisk {
        files: HashMap<PathBuf, Vec<u8>>,
        log: Vec<String>,
        fail_append_at: Option<usize>,
        fail_sync: bool,
        fail_rename: bool,
        appends: usize,
    }

    impl Disk for FakeDisk {
        fn create(&mut self, path: &Path) -> io::Result<()> {
            self.log.push(format!("create {}", path.display()));
            self.files.insert(path.to_path_buf(), Vec::new());
            Ok(())
        }

        fn append(&mut self, path: &Path, data: &[u8]) -> io::Result<()> {
            self.log.push(format!("append {}", path.display()));
            self.appends += 1;
            if self.fail_append_at == Some(self.appends) {
                return Err(io::Error::other("диск отказал"));
            }
            self.files.get_mut(path).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "нет файла"))?.extend_from_slice(data);
            Ok(())
        }

        fn sync(&mut self, path: &Path) -> io::Result<()> {
            self.log.push(format!("sync {}", path.display()));
            if self.fail_sync { Err(io::Error::other("sync не удался")) } else { Ok(()) }
        }

        fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
            self.log.push(format!("rename {} {}", from.display(), to.display()));
            if self.fail_rename {
                return Err(io::Error::other("rename не удался"));
            }
            let data = self.files.remove(from).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "нет файла"))?;
            self.files.insert(to.to_path_buf(), data);
            Ok(())
        }

        fn remove(&mut self, path: &Path) -> io::Result<()> {
            self.log.push(format!("remove {}", path.display()));
            self.files.remove(path);
            Ok(())
        }
    }

    fn dest() -> PathBuf {
        PathBuf::from("/data/ab/ab12cd")
    }

    fn position(disk: &FakeDisk, prefix: &str) -> usize {
        disk.log.iter().position(|l| l.starts_with(prefix)).unwrap_or_else(|| panic!("в журнале нет операции {prefix}: {:?}", disk.log))
    }

    #[test]
    fn the_data_arrives_and_nothing_else_is_left() {
        let mut disk = FakeDisk::default();
        write_atomic(&mut disk, &dest(), &[b"hello ", b"world"], 7).expect("запись");
        assert_eq!(disk.files.get(&dest()).map(Vec::as_slice), Some(&b"hello world"[..]), "по итоговому пути лежит всё содержимое");
        assert_eq!(disk.files.len(), 1, "после успеха рядом нет временных файлов: {:?}", disk.files.keys().collect::<Vec<_>>());
    }

    #[test]
    fn the_final_name_is_never_written_directly() {
        let mut disk = FakeDisk::default();
        write_atomic(&mut disk, &dest(), &[b"abc"], 1).unwrap();
        for op in &disk.log {
            let direct = op.starts_with(&format!("create {}", dest().display()))
                || op.starts_with(&format!("append {}", dest().display()))
                || op.starts_with(&format!("sync {}", dest().display()));
            assert!(!direct, "операция {op:?} трогает итоговое имя: читатель мог бы увидеть недописанное");
        }
    }

    #[test]
    fn data_is_synced_before_the_name_appears() {
        let mut disk = FakeDisk::default();
        write_atomic(&mut disk, &dest(), &[b"a", b"b", b"c"], 1).unwrap();
        let last_append = disk.log.iter().rposition(|l| l.starts_with("append")).unwrap();
        let sync = position(&disk, "sync");
        let rename = position(&disk, "rename");
        assert!(last_append < sync, "sync должен идти после последней записи, журнал: {:?}", disk.log);
        assert!(sync < rename, "без sync перед rename после отключения питания по новому имени может лежать пустой файл, журнал: {:?}", disk.log);
    }

    #[test]
    fn the_temporary_file_is_in_the_same_directory() {
        let mut disk = FakeDisk::default();
        write_atomic(&mut disk, &dest(), &[b"x"], 1).unwrap();
        let rename = disk.log.iter().find(|l| l.starts_with("rename")).unwrap().clone();
        let parts: Vec<&str> = rename.split(' ').collect();
        let (from, to) = (Path::new(parts[1]), Path::new(parts[2]));
        assert_eq!(from.parent(), to.parent(), "rename между разными каталогами (а то и дисками) уже не атомарный: временный файл кладут рядом с итоговым");
        assert_ne!(from, to, "временное имя должно отличаться от итогового");
    }

    #[test]
    fn a_failed_write_keeps_the_old_file_and_leaves_no_litter() {
        let mut disk = FakeDisk::default();
        disk.files.insert(dest(), b"old content".to_vec());
        disk.fail_append_at = Some(2);
        let result = write_atomic(&mut disk, &dest(), &[b"new ", b"content"], 1);
        assert!(result.is_err(), "ошибка записи должна дойти до вызывающего");
        assert_eq!(disk.files.get(&dest()).map(Vec::as_slice), Some(&b"old content"[..]), "старое содержимое осталось как было");
        assert_eq!(disk.files.len(), 1, "временный файл убран: {:?}", disk.files.keys().collect::<Vec<_>>());
    }

    #[test]
    fn a_failed_sync_or_rename_is_cleaned_up_too() {
        let mut disk = FakeDisk { fail_sync: true, ..FakeDisk::default() };
        assert!(write_atomic(&mut disk, &dest(), &[b"x"], 1).is_err(), "ошибка sync должна дойти до вызывающего");
        assert!(disk.files.is_empty(), "после неудачного sync нет ни итогового файла, ни временного: {:?}", disk.files.keys().collect::<Vec<_>>());
        assert!(!disk.log.iter().any(|l| l.starts_with("rename")), "с неподтверждённым sync переименовывать нельзя");

        let mut disk = FakeDisk { fail_rename: true, ..FakeDisk::default() };
        assert!(write_atomic(&mut disk, &dest(), &[b"x"], 1).is_err(), "ошибка rename должна дойти до вызывающего");
        assert!(disk.files.is_empty(), "после неудачного rename временный файл убран: {:?}", disk.files.keys().collect::<Vec<_>>());
    }

    #[test]
    fn two_writers_use_different_temporary_names() {
        let mut disk = FakeDisk::default();
        write_atomic(&mut disk, &dest(), &[b"x"], 1).unwrap();
        write_atomic(&mut disk, &dest(), &[b"y"], 2).unwrap();
        let creates: Vec<&String> = disk.log.iter().filter(|l| l.starts_with("create")).collect();
        assert_ne!(creates[0], creates[1], "параллельные записи одного ключа не должны делить временный файл: имя зависит от nonce");
    }
}
