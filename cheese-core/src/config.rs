use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use xdg::BaseDirectories;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ui: UiConfig,
    pub navigation: NavigationConfig,
    pub performance: PerformanceConfig,
    pub keyboard: KeyboardConfig,
    pub integrations: IntegrationsConfig,
    pub plugins: PluginsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: Theme,
    pub show_hidden: bool,
    pub dual_pane: bool,
    pub icon_size: u32,
    pub font_size: u32,
    pub confirm_delete: bool,
    pub confirm_trash: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Auto,
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationConfig {
    pub follow_symlinks: bool,
    pub max_depth: usize,
    pub sort_by: SortBy,
    pub sort_order: SortOrder,
    pub group_directories: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub cache_size_mb: usize,
    pub thumbnail_cache_mb: usize,
    pub max_concurrent_ops: usize,
    pub debounce_ms: u64,
    pub large_dir_threshold: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardConfig {
    pub vim_mode: bool,
    pub command_palette: String,
    pub fuzzy_search: String,
    pub new_tab: String,
    pub close_tab: String,
    pub toggle_hidden: String,
    pub delete: String,
    pub trash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationsConfig {
    pub terminal: String,
    pub editor: String,
    pub archive_manager: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginsConfig {
    pub enabled: Vec<String>,
    pub auto_update: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig {
                theme: Theme::Auto,
                show_hidden: false,
                dual_pane: false,
                icon_size: 24,
                font_size: 10,
                confirm_delete: true,
                confirm_trash: false,
            },
            navigation: NavigationConfig {
                follow_symlinks: true,
                max_depth: 32,
                sort_by: SortBy::Name,
                sort_order: SortOrder::Ascending,
                group_directories: true,
            },
            performance: PerformanceConfig {
                cache_size_mb: 128,
                thumbnail_cache_mb: 64,
                max_concurrent_ops: 4,
                debounce_ms: 150,
                large_dir_threshold: 10000,
            },
            keyboard: KeyboardConfig {
                vim_mode: true,
                command_palette: "Ctrl+P".to_string(),
                fuzzy_search: "Ctrl+F".to_string(),
                new_tab: "Ctrl+T".to_string(),
                close_tab: "Ctrl+W".to_string(),
                toggle_hidden: "Ctrl+H".to_string(),
                delete: "Delete".to_string(),
                trash: "Shift+Delete".to_string(),
            },
            integrations: IntegrationsConfig {
                terminal: "xfce4-terminal".to_string(),
                editor: "$EDITOR".to_string(),
                archive_manager: "xarchiver".to_string(),
            },
            plugins: PluginsConfig {
                enabled: vec!["git-overlay".to_string(), "archive-preview".to_string()],
                auto_update: false,
            },
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let xdg_dirs = BaseDirectories::with_prefix("cheese")
            .map_err(|e| Error::Config(format!("Failed to get XDG directories: {}", e)))?;

        let config_path = xdg_dirs
            .find_config_file("cheese.toml")
            .unwrap_or_else(|| {
                xdg_dirs.get_config_home().join("cheese.toml")
            });

        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            toml::from_str(&contents).map_err(Into::into)
        } else {
            let default_config = Self::default();
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let toml_str = toml::to_string_pretty(&default_config)
                .map_err(|e| Error::Config(format!("Failed to serialize config: {}", e)))?;
            std::fs::write(&config_path, toml_str)?;
            Ok(default_config)
        }
    }

    pub fn save(&self) -> Result<()> {
        let xdg_dirs = BaseDirectories::with_prefix("cheese")
            .map_err(|e| Error::Config(format!("Failed to get XDG directories: {}", e)))?;

        let config_path = xdg_dirs.get_config_home().join("cheese.toml");
        
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("Failed to serialize config: {}", e)))?;
        
        std::fs::write(&config_path, toml_str)?;
        Ok(())
    }

    pub fn config_path() -> Result<PathBuf> {
        let xdg_dirs = BaseDirectories::with_prefix("cheese")
            .map_err(|e| Error::Config(format!("Failed to get XDG directories: {}", e)))?;
        Ok(xdg_dirs.get_config_home().join("cheese.toml"))
    }
}
