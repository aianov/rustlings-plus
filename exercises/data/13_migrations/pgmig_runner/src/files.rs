use std::path::Path;

use crate::{MigrateError, Migration};

pub fn parse_dir(dir: &Path) -> Result<Vec<Migration>, MigrateError> {
    let _ = dir;
    todo!()
}
