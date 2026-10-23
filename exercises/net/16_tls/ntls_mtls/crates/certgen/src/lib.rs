// Утилита выпуска сертификатов для сервисов: свой CA и по сертификату на сервис.
// Файлы с сертификатами в репозиторий не попадают: .gitignore должен их исключать.
use std::path::Path;

pub fn issue_all(dir: &Path) -> std::io::Result<()> {
    todo!()
}

pub fn ensure_gitignore(repo_root: &Path) -> std::io::Result<()> {
    todo!()
}
