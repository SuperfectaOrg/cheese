pub mod polkit;
pub mod selinux;

use crate::{Error, Result};
use std::path::Path;

pub struct Security {
    polkit: polkit::PolkitClient,
    selinux_enabled: bool,
}

impl Security {
    pub fn new() -> Result<Self> {
        let polkit = polkit::PolkitClient::new()?;
        let selinux_enabled = selinux::is_enabled();

        Ok(Self {
            polkit,
            selinux_enabled,
        })
    }

    pub async fn check_permission(&self, action: &str) -> Result<bool> {
        self.polkit.check_authorization(action).await
    }

    pub async fn request_authorization(&self, action: &str) -> Result<bool> {
        self.polkit.request_authorization(action).await
    }

    pub fn check_selinux_context(&self, path: &Path) -> Result<()> {
        if !self.selinux_enabled {
            return Ok(());
        }

        selinux::check_context(path)
    }

    pub fn is_selinux_enabled(&self) -> bool {
        self.selinux_enabled
    }

    pub fn validate_safe_operation(&self, path: &Path) -> Result<()> {
        if is_running_as_root() {
            return Err(Error::InvalidOperation(
                "Cheese must not be run as root".to_string()
            ));
        }

        if is_system_path(path) {
            return Err(Error::PermissionDenied { path: path.to_path_buf() });
        }

        if self.selinux_enabled {
            self.check_selinux_context(path)?;
        }

        Ok(())
    }
}

impl Default for Security {
    fn default() -> Self {
        Self::new().expect("Failed to initialize security")
    }
}

pub fn is_running_as_root() -> bool {
    #[cfg(unix)]
    {
        use nix::unistd::Uid;
        Uid::effective().is_root()
    }

    #[cfg(not(unix))]
    {
        false
    }
}

pub fn is_system_path(path: &Path) -> bool {
    let system_paths = [
        "/bin",
        "/boot",
        "/dev",
        "/etc",
        "/lib",
        "/lib64",
        "/proc",
        "/root",
        "/sbin",
        "/sys",
        "/usr/bin",
        "/usr/sbin",
        "/usr/lib",
        "/usr/lib64",
    ];

    for system_path in &system_paths {
        if path.starts_with(system_path) {
            return true;
        }
    }

    false
}

pub fn validate_symlink_target(link: &Path, target: &Path) -> Result<()> {
    if target.is_absolute() {
        if is_system_path(target) {
            tracing::warn!("Symlink points to system path: {} -> {}", 
                link.display(), target.display());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_path_detection() {
        assert!(is_system_path(Path::new("/bin/ls")));
        assert!(is_system_path(Path::new("/etc/passwd")));
        assert!(!is_system_path(Path::new("/home/user/file.txt")));
        assert!(!is_system_path(Path::new("/tmp/test")));
    }
}
