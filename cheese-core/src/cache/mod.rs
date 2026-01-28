pub mod lru;
pub mod thumbnail;

use crate::{Error, Result};
use crate::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::RwLock;
use lru::LruCache;
use std::num::NonZeroUsize;

const DEFAULT_CACHE_SIZE: usize = 10000;

#[derive(Clone)]
pub struct MetadataCache {
    cache: Arc<RwLock<LruCache<u64, CachedMetadata>>>,
}

#[derive(Debug, Clone)]
pub struct CachedMetadata {
    pub entry: DirEntry,
    pub cached_at: std::time::Instant,
}

impl MetadataCache {
    pub fn new(capacity_mb: usize) -> Self {
        let size = (capacity_mb * 1024 * 1024) / std::mem::size_of::<CachedMetadata>();
        let capacity = NonZeroUsize::new(size.max(DEFAULT_CACHE_SIZE)).unwrap();
        
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(capacity))),
        }
    }

    pub fn get(&self, inode: u64) -> Option<DirEntry> {
        let mut cache = self.cache.write();
        cache.get(&inode).map(|cached| cached.entry.clone())
    }

    pub fn insert(&self, inode: u64, entry: DirEntry) {
        let mut cache = self.cache.write();
        cache.put(inode, CachedMetadata {
            entry,
            cached_at: std::time::Instant::now(),
        });
    }

    pub fn remove(&self, inode: u64) {
        let mut cache = self.cache.write();
        cache.pop(&inode);
    }

    pub fn get_or_fetch(&self, path: &Path) -> Result<DirEntry> {
        let metadata = std::fs::symlink_metadata(path)?;
        let inode = get_inode(&metadata);

        if let Some(cached) = self.get(inode) {
            if is_valid(&cached, &metadata) {
                return Ok(cached);
            }
        }

        let entry = DirEntry::from_path(path)?;
        self.insert(inode, entry.clone());
        Ok(entry)
    }

    pub fn invalidate(&self, path: &Path) -> Result<()> {
        let metadata = std::fs::symlink_metadata(path)?;
        let inode = get_inode(&metadata);
        self.remove(inode);
        Ok(())
    }

    pub fn invalidate_directory(&self, dir: &Path) -> Result<()> {
        let mut to_remove = Vec::new();
        
        {
            let cache = self.cache.read();
            for (inode, cached) in cache.iter() {
                if cached.entry.path.starts_with(dir) {
                    to_remove.push(*inode);
                }
            }
        }

        let mut cache = self.cache.write();
        for inode in to_remove {
            cache.pop(&inode);
        }

        Ok(())
    }

    pub fn clear(&self) {
        let mut cache = self.cache.write();
        cache.clear();
    }

    pub fn len(&self) -> usize {
        let cache = self.cache.read();
        cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        let cache = self.cache.read();
        cache.cap().get()
    }
}

impl Default for MetadataCache {
    fn default() -> Self {
        Self::new(128)
    }
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

fn is_valid(cached: &DirEntry, metadata: &std::fs::Metadata) -> bool {
    cached.size == metadata.len() &&
    cached.modified == metadata.modified().unwrap_or(std::time::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_cache_basic_operations() {
        let cache = MetadataCache::new(1);
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        let entry = DirEntry::from_path(&file_path).unwrap();
        cache.insert(entry.inode, entry.clone());

        let retrieved = cache.get(entry.inode).unwrap();
        assert_eq!(retrieved.path, file_path);

        cache.remove(entry.inode);
        assert!(cache.get(entry.inode).is_none());
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = MetadataCache::new(1);
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        let entry = cache.get_or_fetch(&file_path).unwrap();
        assert_eq!(cache.len(), 1);

        cache.invalidate(&file_path).unwrap();
        assert_eq!(cache.len(), 0);
    }
}
