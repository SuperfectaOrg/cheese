use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Permission denied: {path}")]
    PermissionDenied { path: PathBuf },

    #[error("Not found: {path}")]
    NotFound { path: PathBuf },

    #[error("Already exists: {path}")]
    AlreadyExists { path: PathBuf },

    #[error("Invalid path: {path}")]
    InvalidPath { path: PathBuf },

    #[error("Symlink loop detected: {path}")]
    SymlinkLoop { path: PathBuf },

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Operation timeout: {0}")]
    Timeout(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("SELinux context error: {0}")]
    SelinuxContext(String),

    #[error("Polkit authorization failed: {0}")]
    PolkitDenied(String),

    #[error("Trash operation failed: {0}")]
    TrashError(String),

    #[error("Mount operation failed: {0}")]
    MountError(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Watcher error: {0}")]
    Watcher(String),

    #[error("D-Bus error: {0}")]
    DBus(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Runtime error: {0}")]
    Runtime(String),
}

impl From<tokio::io::Error> for Error {
    fn from(err: tokio::io::Error) -> Self {
        Error::Io(err.into())
    }
}

impl From<notify::Error> for Error {
    fn from(err: notify::Error) -> Self {
        Error::Watcher(err.to_string())
    }
}

impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Self {
        Error::Config(err.to_string())
    }
}

impl From<zbus::Error> for Error {
    fn from(err: zbus::Error) -> Self {
        Error::DBus(err.to_string())
    }
}
