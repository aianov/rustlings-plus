use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use contracts::ParseError;

use crate::domain::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Import,
    Trends,
    Flush,
    Job(&'static str),
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::Import => write!(f, "импорт"),
            Op::Trends => write!(f, "пересчёт трендов"),
            Op::Flush => write!(f, "доставка уведомлений"),
            Op::Job(name) => write!(f, "{name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    Read { path: PathBuf, kind: io::ErrorKind },
    Parse { line: usize, error: ParseError },
    Unsupported { line: usize },
    UnknownName { line: usize, name: String },
    Record { line: usize, error: DomainError },
    Panicked { line: usize },
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Read { path, kind } => {
                write!(f, "не прочитать {}: {kind}", path.display())
            }
            ImportError::Parse { line, error } => write!(f, "строка {line}: {error}"),
            ImportError::Unsupported { line } => {
                write!(f, "строка {line}: в импорте только register, follow и post")
            }
            ImportError::UnknownName { line, name } => {
                write!(f, "строка {line}: нет пользователя @{name}")
            }
            ImportError::Record { line, error } => write!(f, "строка {line}: {error}"),
            ImportError::Panicked { line } => {
                write!(f, "строка {line}: задача импорта упала с паникой")
            }
        }
    }
}

impl Error for ImportError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    Domain(DomainError),
    Import(ImportError),
    Timeout { op: Op, after: Duration },
    Panicked(Op),
    Stopped(Op),
}

impl From<DomainError> for AppError {
    fn from(error: DomainError) -> AppError {
        AppError::Domain(error)
    }
}

impl From<ImportError> for AppError {
    fn from(error: ImportError) -> AppError {
        AppError::Import(error)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Domain(..) => write!(f, "запрос отклонён"),
            AppError::Import(..) => write!(f, "импорт остановлен"),
            AppError::Timeout { op, after } => {
                write!(f, "{op}: превышено время ожидания ({after:?})")
            }
            AppError::Panicked(op) => write!(f, "{op}: задача упала с паникой"),
            AppError::Stopped(op) => write!(f, "{op}: ядро останавливается"),
        }
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AppError::Domain(error) => Some(error),
            AppError::Import(error) => Some(error),
            AppError::Timeout { .. } | AppError::Panicked(..) | AppError::Stopped(..) => None,
        }
    }
}
