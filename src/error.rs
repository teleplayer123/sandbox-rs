use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("Path escape: '{0}' resolves outside the sandbox root")]
    PathEscape(String),

    #[error("Invalid sandbox directory: {0}")]
    InvalidDir(String),

    #[error("Invalid argument: {0}")]
    InvalidArg(String),
}

pub type Result<T> = std::result::Result<T, SandboxError>;
