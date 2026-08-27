use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
    #[error("{message}")]
    Hint { message: String, hint: String },
    #[error("confirm required: {0}")]
    Confirm(String),
    #[error("io {0}")]
    Io(#[from] std::io::Error),
    #[error("json {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing {0}")]
    Missing(PathBuf),
}

impl Error {
    pub fn hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::Hint { message: message.into(), hint: hint.into() }
    }

    pub fn hint_str(&self) -> Option<&str> {
        match self {
            Self::Hint { hint, .. } | Self::Confirm(hint) => Some(hint.as_str()),
            _ => None,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Confirm(_) => crate::EXIT_CONFIRM,
            _ => 1,
        }
    }
}
