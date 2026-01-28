use crate::{Error, Result};
use crate::fs::{DirEntry, validate_path, check_symlink_loop};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const BATCH_SIZE: usize = 100;

pub struct ScanResult {
    pub entries: Vec<DirEntry>,
    pub total_count: usize,
    pub is_complete: bool,
}

pub struct Scanner {
    follow_symlinks: bool,
    max_depth: usize,
    show_hidden: bool,
}

impl Scanner {
    pub fn new(follow_symlinks: bool, max_depth: usize, show_hidden: bool) -> Self {
        Self {
            follow_symlinks,
            max_depth,
            show_hidden,
        }
    }

    pub async fn scan_directory(
        &self,
        path: PathBuf,
        sender: mpsc::Sender<ScanResult>,
        cancel: CancellationToken,
    ) -> Result<()> {
        validate_path(&path)?;

        let resolved_path = if self.follow_symlinks {
            check_symlink_loop(&path, self.max_depth)?
        } else {
            path.clone()
        };

        if !resolved_path.is_dir() {
            return Err(Error::InvalidPath { path: resolved_path });
        }

        let total_count = Arc::new(AtomicUsize::new(0));
        let mut entries = Vec::with_capacity(BATCH_SIZE);
        
        let mut read_dir = tokio::fs::read_dir(&resolved_path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }

            let entry_path = entry.path();
            
            match DirEntry::from_path(&entry_path) {
                Ok(dir_entry) => {
                    if !self.show_hidden && dir_entry.is_hidden() {
                        continue;
                    }

                    entries.push(dir_entry);
                    total_count.fetch_add(1, Ordering::Relaxed);

                    if entries.len() >= BATCH_SIZE {
                        let batch = std::mem::replace(&mut entries, Vec::with_capacity(BATCH_SIZE));
                        let count = total_count.load(Ordering::Relaxed);
                        
                        sender.send(ScanResult {
                            entries: batch,
                            total_count: count,
                            is_complete: false,
                        }).await.map_err(|_| Error::Cancelled)?;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read entry {:?}: {}", entry_path, e);
                    continue;
                }
            }
        }

        if !entries.is_empty() || total_count.load(Ordering::Relaxed) == 0 {
            let count = total_count.load(Ordering::Relaxed);
            sender.send(ScanResult {
                entries,
                total_count: count,
                is_complete: true,
            }).await.map_err(|_| Error::Cancelled)?;
        }

        Ok(())
    }

    pub async fn scan_recursive(
        &self,
        path: PathBuf,
        sender: mpsc::Sender<ScanResult>,
        cancel: CancellationToken,
    ) -> Result<()> {
        self.scan_recursive_internal(path, 0, sender, cancel).await
    }

    fn scan_recursive_internal(
        &self,
        path: PathBuf,
        depth: usize,
        sender: mpsc::Sender<ScanResult>,
        cancel: CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            if depth >= self.max_depth {
                return Ok(());
            }

            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }

            validate_path(&path)?;

            let resolved_path = if self.follow_symlinks {
                check_symlink_loop(&path, self.max_depth)?
            } else {
                path.clone()
            };

            if !resolved_path.is_dir() {
                return Ok(());
            }

            let mut read_dir = tokio::fs::read_dir(&resolved_path).await?;
            let mut entries = Vec::with_capacity(BATCH_SIZE);
            let mut subdirs = Vec::new();

            while let Some(entry) = read_dir.next_entry().await? {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }

                let entry_path = entry.path();

                match DirEntry::from_path(&entry_path) {
                    Ok(dir_entry) => {
                        if !self.show_hidden && dir_entry.is_hidden() {
                            continue;
                        }

                        if dir_entry.is_dir && !dir_entry.is_symlink {
                            subdirs.push(entry_path.clone());
                        }

                        entries.push(dir_entry);

                        if entries.len() >= BATCH_SIZE {
                            let batch = std::mem::replace(&mut entries, Vec::with_capacity(BATCH_SIZE));
                            sender.send(ScanResult {
                                entries: batch,
                                total_count: 0,
                                is_complete: false,
                            }).await.map_err(|_| Error::Cancelled)?;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read entry {:?}: {}", entry_path, e);
                        continue;
                    }
                }
            }

            if !entries.is_empty() {
                sender.send(ScanResult {
                    entries,
                    total_count: 0,
                    is_complete: false,
                }).await.map_err(|_| Error::Cancelled)?;
            }

            for subdir in subdirs {
                self.scan_recursive_internal(subdir, depth + 1, sender.clone(), cancel.clone()).await?;
            }

            Ok(())
        })
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new(true, 32, false)
    }
}
