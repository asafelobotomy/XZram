use thiserror::Error;

pub type Result<T> = std::result::Result<T, XzramError>;

#[derive(Debug, Error)]
pub enum XzramError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("backend error: {0}")]
    Backend(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("permission denied: {0}")]
    Permission(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("command failed: {0}")]
    Command(String),
}
