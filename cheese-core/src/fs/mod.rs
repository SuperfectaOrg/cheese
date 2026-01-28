pub mod scanner;
pub mod metadata;
pub mod watcher;
pub mod ops;

use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub permissions: u32,
    pub inode: u64,
}

impl DirEntry {
    pub fn from_path(path: &Path) -> Result<Self> {
        let metadata = std::fs::symlink_metadata(path)?;
        let name = path
            .file_name()
            .ok_or_else(|| Error::InvalidPath { path: path.to_path_buf() })?
            .to_string_lossy()
            .into_owned();

        Ok(Self {
            name,
            path: path.to_path_buf(),
            size: metadata.len(),
            modified: metadata.modified()?,
            is_dir: metadata.is_dir(),
            is_symlink: metadata.is_symlink(),
            permissions: get_permissions(&metadata),
            inode: get_inode(&metadata),
        })
    }

    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('.')
    }

    pub fn extension(&self) -> Option<String> {
        self.path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
    }

    pub fn mime_type(&self) -> String {
        mime_guess::from_path(&self.path)
            .first_or_octet_stream()
            .to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
    BlockDevice,
    CharDevice,
    Fifo,
    Socket,
    Unknown,
}

impl EntryType {
    pub fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::FileTypeExt;
        let file_type = metadata.file_type();

        if file_type.is_file() {
            EntryType::File
        } else if file_type.is_dir() {
            EntryType::Directory
        } else if file_type.is_symlink() {
            EntryType::Symlink
        } else if file_type.is_block_device() {
            EntryType::BlockDevice
        } else if file_type.is_char_device() {
            EntryType::CharDevice
        } else if file_type.is_fifo() {
            EntryType::Fifo
        } else if file_type.is_socket() {
            EntryType::Socket
        } else {
            EntryType::Unknown
        }
    }
}

#[cfg(unix)]
fn get_permissions(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode()
}

#[cfg(not(unix))]
fn get_permissions(_metadata: &std::fs::Metadata) -> u32 {
    0
}

#[cfg(unix)]
fn get_inode(metadata: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.ino()
}

#[cfg(not(unix))]
fn get_inode(_metadata: &std::fs::Metadata) -> u64 {
    0
}

pub fn validate_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(Error::NotFound { path: path.to_path_buf() });
    }

    if path.components().count() > 256 {
        return Err(Error::InvalidPath { path: path.to_path_buf() });
    }

    Ok(())
}

pub fn check_symlink_loop(path: &Path, max_depth: usize) -> Result<PathBuf> {
    let mut current = path.to_path_buf();
    let mut depth = 0;

    while current.is_symlink() {
        if depth >= max_depth {
            return Err(Error::SymlinkLoop { path: path.to_path_buf() });
        }

        current = std::fs::read_link(&current)?;
        depth += 1;
    }

    Ok(current)
}
