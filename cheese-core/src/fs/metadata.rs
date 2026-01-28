use crate::{Error, Result};
use crate::fs::DirEntry;
use std::path::Path;
use std::time::SystemTime;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ExtendedMetadata {
    pub entry: DirEntry,
    pub owner: String,
    pub group: String,
    pub link_target: Option<String>,
    pub mime_type: String,
    pub is_executable: bool,
    pub is_readable: bool,
    pub is_writable: bool,
}

impl ExtendedMetadata {
    pub fn from_path(path: &Path) -> Result<Self> {
        let entry = DirEntry::from_path(path)?;
        let metadata = std::fs::symlink_metadata(path)?;

        let (owner, group) = get_owner_group(&metadata);
        let link_target = if metadata.is_symlink() {
            std::fs::read_link(path)
                .ok()
                .and_then(|p| p.to_str().map(String::from))
        } else {
            None
        };

        let mime_type = entry.mime_type();
        let is_executable = is_executable(&metadata);
        let is_readable = is_readable(path);
        let is_writable = is_writable(path);

        Ok(Self {
            entry,
            owner,
            group,
            link_target,
            mime_type,
            is_executable,
            is_readable,
            is_writable,
        })
    }

    pub fn format_size(&self) -> String {
        format_bytes(self.entry.size)
    }

    pub fn format_permissions(&self) -> String {
        format_permissions(self.entry.permissions)
    }

    pub fn format_modified(&self) -> String {
        format_time(self.entry.modified)
    }
}

#[cfg(unix)]
fn get_owner_group(metadata: &std::fs::Metadata) -> (String, String) {
    use std::os::unix::fs::MetadataExt;
    use nix::unistd::{Uid, Gid, User, Group};

    let uid = Uid::from_raw(metadata.uid());
    let gid = Gid::from_raw(metadata.gid());

    let owner = User::from_uid(uid)
        .ok()
        .flatten()
        .map(|u| u.name)
        .unwrap_or_else(|| uid.to_string());

    let group = Group::from_gid(gid)
        .ok()
        .flatten()
        .map(|g| g.name)
        .unwrap_or_else(|| gid.to_string());

    (owner, group)
}

#[cfg(not(unix))]
fn get_owner_group(_metadata: &std::fs::Metadata) -> (String, String) {
    ("unknown".to_string(), "unknown".to_string())
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn is_readable(path: &Path) -> bool {
    std::fs::File::open(path).is_ok()
}

fn is_writable(path: &Path) -> bool {
    use std::fs::OpenOptions;
    OpenOptions::new()
        .write(true)
        .append(true)
        .open(path)
        .is_ok()
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    
    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

pub fn format_permissions(mode: u32) -> String {
    #[cfg(unix)]
    {
        let user = [
            if mode & 0o400 != 0 { 'r' } else { '-' },
            if mode & 0o200 != 0 { 'w' } else { '-' },
            if mode & 0o100 != 0 { 'x' } else { '-' },
        ];
        let group = [
            if mode & 0o040 != 0 { 'r' } else { '-' },
            if mode & 0o020 != 0 { 'w' } else { '-' },
            if mode & 0o010 != 0 { 'x' } else { '-' },
        ];
        let other = [
            if mode & 0o004 != 0 { 'r' } else { '-' },
            if mode & 0o002 != 0 { 'w' } else { '-' },
            if mode & 0o001 != 0 { 'x' } else { '-' },
        ];

        format!("{}{}{}",
            user.iter().collect::<String>(),
            group.iter().collect::<String>(),
            other.iter().collect::<String>()
        )
    }

    #[cfg(not(unix))]
    {
        format!("{:o}", mode)
    }
}

pub fn format_time(time: SystemTime) -> String {
    use chrono::{DateTime, Local};
    
    let datetime: DateTime<Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub struct MetadataCollector {
    cache: HashMap<u64, ExtendedMetadata>,
}

impl MetadataCollector {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn collect(&mut self, path: &Path) -> Result<ExtendedMetadata> {
        let metadata = ExtendedMetadata::from_path(path)?;
        self.cache.insert(metadata.entry.inode, metadata.clone());
        Ok(metadata)
    }

    pub fn get(&self, inode: u64) -> Option<&ExtendedMetadata> {
        self.cache.get(&inode)
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for MetadataCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_format_permissions() {
        assert_eq!(format_permissions(0o755), "rwxr-xr-x");
        assert_eq!(format_permissions(0o644), "rw-r--r--");
        assert_eq!(format_permissions(0o777), "rwxrwxrwx");
    }

    #[test]
    fn test_extended_metadata() {
        let result = ExtendedMetadata::from_path(Path::new("/tmp"));
        assert!(result.is_ok());
    }
}
