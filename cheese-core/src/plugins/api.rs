use std::path::PathBuf;
use serde::{Deserialize, Serialize};

pub const API_VERSION: u32 = 1;

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub api_version: u32,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub capabilities: Vec<Capability>,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    FilePreview,
    ContextMenu,
    FileOverlay,
    CustomColumn,
    SearchProvider,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContext {
    pub path: PathBuf,
    pub is_directory: bool,
    pub size: u64,
    pub mime_type: String,
    pub permissions: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewRequest {
    pub file: FileContext,
    pub max_width: u32,
    pub max_height: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewResponse {
    pub content: PreviewContent,
    pub cacheable: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreviewContent {
    Text(String),
    Image(Vec<u8>),
    Html(String),
    None,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItem {
    pub label: String,
    pub action: String,
    pub icon: Option<String>,
    pub enabled: bool,
    pub separator_after: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMenuRequest {
    pub files: Vec<FileContext>,
    pub current_directory: PathBuf,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMenuResponse {
    pub items: Vec<MenuItem>,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayRequest {
    pub file: FileContext,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayResponse {
    pub icon: Option<String>,
    pub badge_text: Option<String>,
    pub badge_color: Option<String>,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    pub id: String,
    pub label: String,
    pub width: u32,
    pub sortable: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnValueRequest {
    pub file: FileContext,
    pub column_id: String,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnValueResponse {
    pub value: String,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub directory: PathBuf,
    pub max_results: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: PathBuf,
    pub score: f64,
    pub snippet: Option<String>,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
}

pub trait PluginInterface: Send + Sync {
    fn info(&self) -> PluginInfo;
    
    fn initialize(&mut self) -> Result<(), String>;
    
    fn shutdown(&mut self) -> Result<(), String>;
    
    fn preview(&self, request: PreviewRequest) -> Result<PreviewResponse, String> {
        let _ = request;
        Err("Not implemented".to_string())
    }
    
    fn context_menu(&self, request: ContextMenuRequest) -> Result<ContextMenuResponse, String> {
        let _ = request;
        Err("Not implemented".to_string())
    }
    
    fn overlay(&self, request: OverlayRequest) -> Result<OverlayResponse, String> {
        let _ = request;
        Err("Not implemented".to_string())
    }
    
    fn custom_columns(&self) -> Result<Vec<ColumnDefinition>, String> {
        Err("Not implemented".to_string())
    }
    
    fn column_value(&self, request: ColumnValueRequest) -> Result<ColumnValueResponse, String> {
        let _ = request;
        Err("Not implemented".to_string())
    }
    
    fn search(&self, request: SearchRequest) -> Result<SearchResponse, String> {
        let _ = request;
        Err("Not implemented".to_string())
    }
}

#[macro_export]
macro_rules! export_plugin {
    ($plugin_type:ty) => {
        #[no_mangle]
        pub extern "C" fn _plugin_create() -> *mut dyn $crate::plugins::api::PluginInterface {
            let plugin = Box::new(<$plugin_type>::default());
            Box::into_raw(plugin)
        }

        #[no_mangle]
        pub extern "C" fn _plugin_destroy(ptr: *mut dyn $crate::plugins::api::PluginInterface) {
            if !ptr.is_null() {
                unsafe {
                    let _ = Box::from_raw(ptr);
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPlugin;

    impl PluginInterface for TestPlugin {
        fn info(&self) -> PluginInfo {
            PluginInfo {
                api_version: API_VERSION,
                name: "Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "Test plugin".to_string(),
                author: "Test Author".to_string(),
                capabilities: vec![Capability::FilePreview],
            }
        }

        fn initialize(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn shutdown(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn test_plugin_info() {
        let plugin = TestPlugin;
        let info = plugin.info();
        assert_eq!(info.name, "Test Plugin");
        assert_eq!(info.api_version, API_VERSION);
    }
}
