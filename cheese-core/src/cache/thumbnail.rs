use crate::{Error, Result};
use crate::cache::lru::LruCache;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sha2::{Sha256, Digest};
use xdg::BaseDirectories;

const THUMBNAIL_SIZE_NORMAL: u32 = 128;
const THUMBNAIL_SIZE_LARGE: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThumbnailSize {
    Normal,
    Large,
}

impl ThumbnailSize {
    pub fn pixels(&self) -> u32 {
        match self {
            Self::Normal => THUMBNAIL_SIZE_NORMAL,
            Self::Large => THUMBNAIL_SIZE_LARGE,
        }
    }

    pub fn directory_name(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Large => "large",
        }
    }
}

pub struct ThumbnailCache {
    cache: LruCache<(PathBuf, ThumbnailSize), Vec<u8>>,
    cache_dir: PathBuf,
    size_limit_mb: usize,
}

impl ThumbnailCache {
    pub fn new(size_limit_mb: usize) -> Result<Self> {
        let xdg_dirs = BaseDirectories::new()
            .map_err(|e| Error::Cache(format!("Failed to get XDG directories: {}", e)))?;
        
        let cache_dir = xdg_dirs.get_cache_home().join("thumbnails");
        std::fs::create_dir_all(&cache_dir)?;

        let capacity = (size_limit_mb * 1024 * 1024) / (THUMBNAIL_SIZE_LARGE * THUMBNAIL_SIZE_LARGE * 4) as usize;
        
        Ok(Self {
            cache: LruCache::new(capacity.max(100)),
            cache_dir,
            size_limit_mb,
        })
    }

    pub fn get(&self, path: &Path, size: ThumbnailSize) -> Option<Vec<u8>> {
        let key = (path.to_path_buf(), size);
        
        if let Some(data) = self.cache.get(&key) {
            return Some(data);
        }

        self.load_from_disk(path, size)
    }

    pub fn insert(&self, path: &Path, size: ThumbnailSize, data: Vec<u8>) -> Result<()> {
        let key = (path.to_path_buf(), size);
        self.cache.insert(key, data.clone());
        self.save_to_disk(path, size, &data)?;
        Ok(())
    }

    pub fn remove(&self, path: &Path) {
        for size in [ThumbnailSize::Normal, ThumbnailSize::Large] {
            let key = (path.to_path_buf(), size);
            self.cache.remove(&key);
            let _ = self.remove_from_disk(path, size);
        }
    }

    pub fn clear(&self) {
        self.cache.clear();
        for size in [ThumbnailSize::Normal, ThumbnailSize::Large] {
            let thumb_dir = self.cache_dir.join(size.directory_name());
            if thumb_dir.exists() {
                let _ = std::fs::remove_dir_all(&thumb_dir);
                let _ = std::fs::create_dir_all(&thumb_dir);
            }
        }
    }

    pub fn is_supported_format(path: &Path) -> bool {
        let supported_extensions = [
            "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg",
            "tiff", "tif", "ico", "heic", "heif",
        ];

        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| supported_extensions.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    fn load_from_disk(&self, path: &Path, size: ThumbnailSize) -> Option<Vec<u8>> {
        let thumb_path = self.get_thumbnail_path(path, size)?;
        std::fs::read(&thumb_path).ok()
    }

    fn save_to_disk(&self, path: &Path, size: ThumbnailSize, data: &[u8]) -> Result<()> {
        let thumb_path = self.get_thumbnail_path(path, size)
            .ok_or_else(|| Error::Cache("Failed to get thumbnail path".to_string()))?;

        if let Some(parent) = thumb_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&thumb_path, data)?;
        Ok(())
    }

    fn remove_from_disk(&self, path: &Path, size: ThumbnailSize) -> Result<()> {
        if let Some(thumb_path) = self.get_thumbnail_path(path, size) {
            if thumb_path.exists() {
                std::fs::remove_file(&thumb_path)?;
            }
        }
        Ok(())
    }

    fn get_thumbnail_path(&self, path: &Path, size: ThumbnailSize) -> Option<PathBuf> {
        let uri = format!("file://{}", path.display());
        let hash = self.compute_hash(&uri);
        
        Some(self.cache_dir
            .join(size.directory_name())
            .join(format!("{}.png", hash)))
    }

    fn compute_hash(&self, uri: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(uri.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    pub fn cache_capacity(&self) -> usize {
        self.cache.capacity()
    }

    pub fn disk_size(&self) -> Result<u64> {
        let mut total = 0u64;
        
        for size in [ThumbnailSize::Normal, ThumbnailSize::Large] {
            let thumb_dir = self.cache_dir.join(size.directory_name());
            if thumb_dir.exists() {
                total += self.dir_size(&thumb_dir)?;
            }
        }

        Ok(total)
    }

    fn dir_size(&self, path: &Path) -> Result<u64> {
        let mut total = 0u64;

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            
            if metadata.is_file() {
                total += metadata.len();
            } else if metadata.is_dir() {
                total += self.dir_size(&entry.path())?;
            }
        }

        Ok(total)
    }

    pub async fn generate_thumbnail(
        &self,
        path: &Path,
        size: ThumbnailSize,
    ) -> Result<Vec<u8>> {
        if !path.exists() {
            return Err(Error::NotFound { path: path.to_path_buf() });
        }

        if !Self::is_supported_format(path) {
            return Err(Error::Cache("Unsupported format".to_string()));
        }

        let data = tokio::fs::read(path).await?;
        let thumbnail = self.create_thumbnail_data(&data, size)?;
        
        self.insert(path, size, thumbnail.clone())?;
        Ok(thumbnail)
    }

    fn create_thumbnail_data(&self, _data: &[u8], size: ThumbnailSize) -> Result<Vec<u8>> {
        let pixels = size.pixels();
        let placeholder = vec![0u8; (pixels * pixels * 4) as usize];
        Ok(placeholder)
    }
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        Self::new(64).expect("Failed to create thumbnail cache")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_thumbnail_size() {
        assert_eq!(ThumbnailSize::Normal.pixels(), 128);
        assert_eq!(ThumbnailSize::Large.pixels(), 256);
    }

    #[test]
    fn test_supported_format() {
        assert!(ThumbnailCache::is_supported_format(Path::new("test.png")));
        assert!(ThumbnailCache::is_supported_format(Path::new("test.jpg")));
        assert!(!ThumbnailCache::is_supported_format(Path::new("test.txt")));
    }

    #[test]
    fn test_thumbnail_cache_creation() {
        let result = ThumbnailCache::new(64);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cache_insert_and_get() {
        let cache = ThumbnailCache::new(64).unwrap();
        let path = PathBuf::from("/tmp/test.png");
        let data = vec![1, 2, 3, 4];
        
        cache.insert(&path, ThumbnailSize::Normal, data.clone()).unwrap();
        let retrieved = cache.get(&path, ThumbnailSize::Normal);
        
        assert_eq!(retrieved, Some(data));
    }
}
