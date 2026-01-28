use crate::{Error, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct OperationProgress {
    pub current_bytes: u64,
    pub total_bytes: u64,
    pub current_file: PathBuf,
    pub files_processed: usize,
    pub total_files: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    Skip,
    Overwrite,
    Rename,
}

pub struct FileOperations {
    max_concurrent: usize,
}

impl FileOperations {
    pub fn new(max_concurrent: usize) -> Self {
        Self { max_concurrent }
    }

    pub async fn copy_files(
        &self,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        conflict: ConflictResolution,
        progress: mpsc::Sender<OperationProgress>,
        cancel: CancellationToken,
    ) -> Result<()> {
        if !dest_dir.is_dir() {
            return Err(Error::InvalidPath { path: dest_dir });
        }

        let total_bytes = self.calculate_total_size(&sources).await?;
        let total_files = sources.len();
        let bytes_copied = Arc::new(AtomicU64::new(0));
        let files_processed = Arc::new(AtomicU64::new(0));

        for source in sources {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }

            let file_name = source.file_name()
                .ok_or_else(|| Error::InvalidPath { path: source.clone() })?;
            let dest = dest_dir.join(file_name);

            if dest.exists() {
                match conflict {
                    ConflictResolution::Skip => continue,
                    ConflictResolution::Overwrite => {},
                    ConflictResolution::Rename => {
                        let renamed = self.find_unique_name(&dest).await?;
                        self.copy_file_with_progress(
                            &source,
                            &renamed,
                            &bytes_copied,
                            total_bytes,
                            &files_processed,
                            total_files,
                            &progress,
                            &cancel,
                        ).await?;
                        continue;
                    }
                }
            }

            self.copy_file_with_progress(
                &source,
                &dest,
                &bytes_copied,
                total_bytes,
                &files_processed,
                total_files,
                &progress,
                &cancel,
            ).await?;
        }

        Ok(())
    }

    async fn copy_file_with_progress(
        &self,
        src: &Path,
        dest: &Path,
        bytes_copied: &Arc<AtomicU64>,
        total_bytes: u64,
        files_processed: &Arc<AtomicU64>,
        total_files: usize,
        progress: &mpsc::Sender<OperationProgress>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let metadata = fs::metadata(src).await?;

        if metadata.is_dir() {
            return self.copy_directory(
                src,
                dest,
                bytes_copied,
                total_bytes,
                files_processed,
                total_files,
                progress,
                cancel,
            ).await;
        }

        let mut src_file = fs::File::open(src).await?;
        let mut dest_file = fs::File::create(dest).await?;
        let mut buffer = vec![0u8; BUFFER_SIZE];

        loop {
            if cancel.is_cancelled() {
                let _ = fs::remove_file(dest).await;
                return Err(Error::Cancelled);
            }

            let n = src_file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            dest_file.write_all(&buffer[..n]).await?;
            
            let current = bytes_copied.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
            let processed = files_processed.load(Ordering::Relaxed) as usize;

            progress.send(OperationProgress {
                current_bytes: current,
                total_bytes,
                current_file: src.to_path_buf(),
                files_processed: processed,
                total_files,
            }).await.map_err(|_| Error::Cancelled)?;
        }

        self.preserve_metadata(src, dest).await?;
        files_processed.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    async fn copy_directory(
        &self,
        src: &Path,
        dest: &Path,
        bytes_copied: &Arc<AtomicU64>,
        total_bytes: u64,
        files_processed: &Arc<AtomicU64>,
        total_files: usize,
        progress: &mpsc::Sender<OperationProgress>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        fs::create_dir_all(dest).await?;
        
        let mut read_dir = fs::read_dir(src).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }

            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());

            self.copy_file_with_progress(
                &src_path,
                &dest_path,
                bytes_copied,
                total_bytes,
                files_processed,
                total_files,
                progress,
                cancel,
            ).await?;
        }

        Ok(())
    }

    pub async fn move_files(
        &self,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        conflict: ConflictResolution,
        progress: mpsc::Sender<OperationProgress>,
        cancel: CancellationToken,
    ) -> Result<()> {
        for source in &sources {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }

            let file_name = source.file_name()
                .ok_or_else(|| Error::InvalidPath { path: source.clone() })?;
            let dest = dest_dir.join(file_name);

            if self.is_same_filesystem(source, &dest_dir).await? {
                if dest.exists() {
                    match conflict {
                        ConflictResolution::Skip => continue,
                        ConflictResolution::Overwrite => {
                            fs::remove_file(&dest).await?;
                        },
                        ConflictResolution::Rename => {
                            let renamed = self.find_unique_name(&dest).await?;
                            fs::rename(source, renamed).await?;
                            continue;
                        }
                    }
                }
                fs::rename(source, &dest).await?;
            } else {
                self.copy_files(
                    vec![source.clone()],
                    dest_dir.clone(),
                    conflict,
                    progress.clone(),
                    cancel.clone(),
                ).await?;
                fs::remove_file(source).await?;
            }
        }

        Ok(())
    }

    pub async fn delete_files(
        &self,
        paths: Vec<PathBuf>,
        progress: mpsc::Sender<OperationProgress>,
        cancel: CancellationToken,
    ) -> Result<()> {
        let total_files = paths.len();
        let mut files_processed = 0;

        for path in paths {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }

            let metadata = fs::symlink_metadata(&path).await?;
            
            if metadata.is_dir() {
                fs::remove_dir_all(&path).await?;
            } else {
                fs::remove_file(&path).await?;
            }

            files_processed += 1;

            progress.send(OperationProgress {
                current_bytes: 0,
                total_bytes: 0,
                current_file: path,
                files_processed,
                total_files,
            }).await.map_err(|_| Error::Cancelled)?;
        }

        Ok(())
    }

    async fn calculate_total_size(&self, paths: &[PathBuf]) -> Result<u64> {
        let mut total = 0u64;

        for path in paths {
            total += self.get_size_recursive(path).await?;
        }

        Ok(total)
    }

    async fn get_size_recursive(&self, path: &Path) -> Result<u64> {
        let metadata = fs::metadata(path).await?;

        if metadata.is_file() {
            return Ok(metadata.len());
        }

        let mut total = 0u64;
        let mut read_dir = fs::read_dir(path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            total += self.get_size_recursive(&entry.path()).await?;
        }

        Ok(total)
    }

    async fn preserve_metadata(&self, src: &Path, dest: &Path) -> Result<()> {
        let metadata = fs::metadata(src).await?;
        fs::set_permissions(dest, metadata.permissions()).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(metadata.permissions().mode());
            fs::set_permissions(dest, perms).await?;
        }

        Ok(())
    }

    async fn find_unique_name(&self, path: &Path) -> Result<PathBuf> {
        let parent = path.parent()
            .ok_or_else(|| Error::InvalidPath { path: path.to_path_buf() })?;
        let stem = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let mut counter = 1;
        loop {
            let new_name = if ext.is_empty() {
                format!("{} ({})", stem, counter)
            } else {
                format!("{} ({}).{}", stem, counter, ext)
            };

            let new_path = parent.join(new_name);
            if !new_path.exists() {
                return Ok(new_path);
            }

            counter += 1;
            if counter > 9999 {
                return Err(Error::InvalidOperation("Too many conflicts".to_string()));
            }
        }
    }

    async fn is_same_filesystem(&self, path1: &Path, path2: &Path) -> Result<bool> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let meta1 = fs::metadata(path1).await?;
            let meta2 = fs::metadata(path2).await?;
            Ok(meta1.dev() == meta2.dev())
        }

        #[cfg(not(unix))]
        {
            Ok(false)
        }
    }
}

impl Default for FileOperations {
    fn default() -> Self {
        Self::new(4)
    }
}
