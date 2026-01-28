pub mod loader;
pub mod api;

use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use parking_lot::RwLock;
use std::sync::Arc;

pub const PLUGIN_API_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub api_version: u32,
    pub capabilities: Vec<String>,
}

pub trait Plugin: Send + Sync {
    fn metadata(&self) -> PluginMetadata;
    fn initialize(&mut self) -> Result<()>;
    fn shutdown(&mut self) -> Result<()>;
}

pub struct PluginManager {
    plugins: Arc<RwLock<HashMap<String, Box<dyn Plugin>>>>,
    plugin_dir: PathBuf,
}

impl PluginManager {
    pub fn new(plugin_dir: PathBuf) -> Result<Self> {
        if !plugin_dir.exists() {
            std::fs::create_dir_all(&plugin_dir)?;
        }

        Ok(Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            plugin_dir,
        })
    }

    pub fn load_plugin(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(Error::NotFound { path: path.to_path_buf() });
        }

        if !self.is_valid_plugin(path)? {
            return Err(Error::Plugin(format!(
                "Invalid plugin file: {}",
                path.display()
            )));
        }

        tracing::info!("Loading plugin from: {}", path.display());
        
        Ok(())
    }

    pub fn unload_plugin(&self, name: &str) -> Result<()> {
        let mut plugins = self.plugins.write();
        
        if let Some(mut plugin) = plugins.remove(name) {
            plugin.shutdown()?;
            tracing::info!("Unloaded plugin: {}", name);
            Ok(())
        } else {
            Err(Error::Plugin(format!("Plugin not found: {}", name)))
        }
    }

    pub fn get_plugin(&self, name: &str) -> Option<PluginMetadata> {
        let plugins = self.plugins.read();
        plugins.get(name).map(|p| p.metadata())
    }

    pub fn list_plugins(&self) -> Vec<PluginMetadata> {
        let plugins = self.plugins.read();
        plugins.values().map(|p| p.metadata()).collect()
    }

    pub fn discover_plugins(&self) -> Result<Vec<PathBuf>> {
        let mut plugin_paths = Vec::new();

        if !self.plugin_dir.exists() {
            return Ok(plugin_paths);
        }

        for entry in std::fs::read_dir(&self.plugin_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("so") {
                plugin_paths.push(path);
            }
        }

        Ok(plugin_paths)
    }

    pub fn load_all_plugins(&self) -> Result<()> {
        let plugin_paths = self.discover_plugins()?;

        for path in plugin_paths {
            match self.load_plugin(&path) {
                Ok(_) => tracing::info!("Successfully loaded: {}", path.display()),
                Err(e) => tracing::warn!("Failed to load {}: {}", path.display(), e),
            }
        }

        Ok(())
    }

    pub fn shutdown_all(&self) -> Result<()> {
        let mut plugins = self.plugins.write();
        
        for (name, mut plugin) in plugins.drain() {
            if let Err(e) = plugin.shutdown() {
                tracing::error!("Failed to shutdown plugin {}: {}", name, e);
            }
        }

        Ok(())
    }

    fn is_valid_plugin(&self, path: &Path) -> Result<bool> {
        if !path.exists() {
            return Ok(false);
        }

        let metadata = std::fs::metadata(path)?;
        
        if !metadata.is_file() {
            return Ok(false);
        }

        if path.extension().and_then(|s| s.to_str()) != Some("so") {
            return Ok(false);
        }

        Ok(true)
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.read().len()
    }

    pub fn is_loaded(&self, name: &str) -> bool {
        self.plugins.read().contains_key(name)
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("cheese")
            .expect("Failed to get XDG directories");
        let plugin_dir = xdg_dirs.get_data_home().join("plugins");
        
        Self::new(plugin_dir).expect("Failed to create plugin manager")
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        let _ = self.shutdown_all();
    }
}

#[derive(Debug, Clone)]
pub enum PluginCapability {
    FilePreview,
    ContextMenu,
    FileOverlay,
    CustomColumn,
    SearchProvider,
}

impl PluginCapability {
    pub fn as_str(&self) -> &str {
        match self {
            Self::FilePreview => "file_preview",
            Self::ContextMenu => "context_menu",
            Self::FileOverlay => "file_overlay",
            Self::CustomColumn => "custom_column",
            Self::SearchProvider => "search_provider",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "file_preview" => Some(Self::FilePreview),
            "context_menu" => Some(Self::ContextMenu),
            "file_overlay" => Some(Self::FileOverlay),
            "custom_column" => Some(Self::CustomColumn),
            "search_provider" => Some(Self::SearchProvider),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_plugin_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PluginManager::new(temp_dir.path().to_path_buf());
        assert!(manager.is_ok());
    }

    #[test]
    fn test_plugin_discovery() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PluginManager::new(temp_dir.path().to_path_buf()).unwrap();
        
        std::fs::write(temp_dir.path().join("test.so"), b"fake").unwrap();
        
        let plugins = manager.discover_plugins().unwrap();
        assert_eq!(plugins.len(), 1);
    }
}
