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

impl XzramError {
    /// The same error with ` (note)` appended to its message, keeping its kind (callers
    /// classify by variant: validation vs I/O vs command failures).
    pub fn with_note(self, note: &str) -> Self {
        match self {
            Self::Io(e) => Self::Io(std::io::Error::new(e.kind(), format!("{e} ({note})"))),
            Self::Parse(m) => Self::Parse(format!("{m} ({note})")),
            Self::Config(m) => Self::Config(format!("{m} ({note})")),
            Self::Backend(m) => Self::Backend(format!("{m} ({note})")),
            Self::NotFound(m) => Self::NotFound(format!("{m} ({note})")),
            Self::Permission(m) => Self::Permission(format!("{m} ({note})")),
            Self::Validation(m) => Self::Validation(format!("{m} ({note})")),
            Self::Command(m) => Self::Command(format!("{m} ({note})")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_note_keeps_the_kind() {
        let e = XzramError::Validation("refused".into()).with_note("nothing changed");
        assert!(matches!(e, XzramError::Validation(ref m) if m == "refused (nothing changed)"));
        let io = XzramError::Io(std::io::Error::other("disk")).with_note("n");
        assert!(matches!(io, XzramError::Io(ref e) if e.to_string() == "disk (n)"));
    }
}
