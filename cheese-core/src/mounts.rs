use crate::{Error, Result};
use zbus::{Connection, proxy};
use std::path::PathBuf;
use std::collections::HashMap;

const UDISKS2_SERVICE: &str = "org.freedesktop.UDisks2";
const UDISKS2_PATH: &str = "/org/freedesktop/UDisks2";

#[proxy(
    interface = "org.freedesktop.UDisks2.Manager",
    default_service = "org.freedesktop.UDisks2",
    default_path = "/org/freedesktop/UDisks2/Manager"
)]
trait UDisks2Manager {
    async fn get_block_devices(&self, options: HashMap<String, zbus::zvariant::Value<'_>>)
        -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.UDisks2.Filesystem",
    default_service = "org.freedesktop.UDisks2"
)]
trait UDisks2Filesystem {
    async fn mount(&self, options: HashMap<String, zbus::zvariant::Value<'_>>)
        -> zbus::Result<String>;
    
    async fn unmount(&self, options: HashMap<String, zbus::zvariant::Value<'_>>)
        -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.UDisks2.Block",
    default_service = "org.freedesktop.UDisks2"
)]
trait UDisks2Block {
    #[zbus(property)]
    async fn device(&self) -> zbus::Result<Vec<u8>>;
    
    #[zbus(property)]
    async fn id_label(&self) -> zbus::Result<String>;
    
    #[zbus(property)]
    async fn id_type(&self) -> zbus::Result<String>;
    
    #[zbus(property)]
    async fn size(&self) -> zbus::Result<u64>;
}

pub struct MountManager {
    connection: Connection,
}

#[derive(Debug, Clone)]
pub struct MountPoint {
    pub device: String,
    pub mount_path: PathBuf,
    pub label: String,
    pub filesystem_type: String,
    pub size: u64,
    pub is_mounted: bool,
}

impl MountManager {
    pub async fn new() -> Result<Self> {
        let connection = Connection::system()
            .await
            .map_err(|e| Error::DBus(format!("Failed to connect to system bus: {}", e)))?;

        Ok(Self { connection })
    }

    pub async fn list_devices(&self) -> Result<Vec<MountPoint>> {
        let manager = UDisks2ManagerProxy::new(&self.connection)
            .await
            .map_err(|e| Error::DBus(format!("Failed to create manager proxy: {}", e)))?;

        let options = HashMap::new();
        let block_devices = manager.get_block_devices(options)
            .await
            .map_err(|e| Error::DBus(format!("Failed to get block devices: {}", e)))?;

        let mut devices = Vec::new();

        for path in block_devices {
            match self.get_device_info(&path).await {
                Ok(Some(device)) => devices.push(device),
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!("Failed to get device info for {:?}: {}", path, e);
                    continue;
                }
            }
        }

        Ok(devices)
    }

    async fn get_device_info(&self, path: &zbus::zvariant::OwnedObjectPath) -> Result<Option<MountPoint>> {
        let block_proxy = UDisks2BlockProxy::builder(&self.connection)
            .path(path.as_ref())
            .map_err(|e| Error::DBus(format!("Invalid path: {}", e)))?
            .build()
            .await
            .map_err(|e| Error::DBus(format!("Failed to create block proxy: {}", e)))?;

        let device_bytes = block_proxy.device().await
            .map_err(|e| Error::DBus(format!("Failed to get device: {}", e)))?;
        let device = String::from_utf8_lossy(&device_bytes)
            .trim_end_matches('\0')
            .to_string();

        if device.is_empty() {
            return Ok(None);
        }

        let label = block_proxy.id_label().await.unwrap_or_default();
        let fs_type = block_proxy.id_type().await.unwrap_or_default();
        let size = block_proxy.size().await.unwrap_or(0);

        if fs_type.is_empty() {
            return Ok(None);
        }

        let mount_path = self.get_mount_path(&device)?;
        let is_mounted = mount_path.exists();

        Ok(Some(MountPoint {
            device,
            mount_path,
            label: if label.is_empty() {
                "Unnamed Device".to_string()
            } else {
                label
            },
            filesystem_type: fs_type,
            size,
            is_mounted,
        }))
    }

    pub async fn mount(&self, device: &str) -> Result<PathBuf> {
        let device_path = self.find_device_path(device).await?;

        let fs_proxy = UDisks2FilesystemProxy::builder(&self.connection)
            .path(device_path.as_ref())
            .map_err(|e| Error::MountError(format!("Invalid path: {}", e)))?
            .build()
            .await
            .map_err(|e| Error::MountError(format!("Failed to create filesystem proxy: {}", e)))?;

        let options = HashMap::new();
        let mount_path = fs_proxy.mount(options)
            .await
            .map_err(|e| Error::MountError(format!("Mount failed: {}", e)))?;

        Ok(PathBuf::from(mount_path))
    }

    pub async fn unmount(&self, device: &str) -> Result<()> {
        let device_path = self.find_device_path(device).await?;

        let fs_proxy = UDisks2FilesystemProxy::builder(&self.connection)
            .path(device_path.as_ref())
            .map_err(|e| Error::MountError(format!("Invalid path: {}", e)))?
            .build()
            .await
            .map_err(|e| Error::MountError(format!("Failed to create filesystem proxy: {}", e)))?;

        let options = HashMap::new();
        fs_proxy.unmount(options)
            .await
            .map_err(|e| Error::MountError(format!("Unmount failed: {}", e)))?;

        Ok(())
    }

    async fn find_device_path(&self, device: &str) -> Result<zbus::zvariant::OwnedObjectPath> {
        let manager = UDisks2ManagerProxy::new(&self.connection)
            .await
            .map_err(|e| Error::DBus(format!("Failed to create manager proxy: {}", e)))?;

        let options = HashMap::new();
        let block_devices = manager.get_block_devices(options)
            .await
            .map_err(|e| Error::DBus(format!("Failed to get block devices: {}", e)))?;

        for path in block_devices {
            let block_proxy = UDisks2BlockProxy::builder(&self.connection)
                .path(path.as_ref())
                .map_err(|e| Error::DBus(format!("Invalid path: {}", e)))?
                .build()
                .await
                .map_err(|e| Error::DBus(format!("Failed to create block proxy: {}", e)))?;

            if let Ok(device_bytes) = block_proxy.device().await {
                let dev = String::from_utf8_lossy(&device_bytes)
                    .trim_end_matches('\0')
                    .to_string();
                
                if dev == device {
                    return Ok(path);
                }
            }
        }

        Err(Error::NotFound { path: PathBuf::from(device) })
    }

    fn get_mount_path(&self, device: &str) -> Result<PathBuf> {
        let mounts = std::fs::read_to_string("/proc/mounts")
            .map_err(|e| Error::MountError(format!("Failed to read /proc/mounts: {}", e)))?;

        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[0] == device {
                return Ok(PathBuf::from(parts[1]));
            }
        }

        Ok(PathBuf::from("/run/media").join(
            std::env::var("USER").unwrap_or_else(|_| "user".to_string())
        ))
    }
}
