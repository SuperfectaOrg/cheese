use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use notify::{Event, EventKind, RecursiveMode, Watcher as NotifyWatcher};
use parking_lot::Mutex;
use std::sync::Arc;

const DEBOUNCE_DURATION: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

pub struct Watcher {
    inner: Arc<Mutex<Option<notify::RecommendedWatcher>>>,
    watched_paths: Arc<Mutex<HashMap<PathBuf, Instant>>>,
    debounce_duration: Duration,
}

impl Watcher {
    pub fn new(debounce_duration: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            watched_paths: Arc::new(Mutex::new(HashMap::new())),
            debounce_duration,
        }
    }

    pub fn start(&self, sender: mpsc::UnboundedSender<WatchEvent>) -> Result<()> {
        let watched_paths = Arc::clone(&self.watched_paths);
        let debounce_duration = self.debounce_duration;

        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            match res {
                Ok(event) => {
                    if let Some(watch_event) = Self::convert_event(event, &watched_paths, debounce_duration) {
                        let _ = sender.send(watch_event);
                    }
                }
                Err(e) => {
                    tracing::error!("Watcher error: {}", e);
                }
            }
        })?;

        *self.inner.lock() = Some(watcher);
        Ok(())
    }

    pub fn watch(&self, path: &Path) -> Result<()> {
        let mut watcher = self.inner.lock();
        
        if let Some(w) = watcher.as_mut() {
            w.watch(path, RecursiveMode::NonRecursive)?;
            self.watched_paths.lock().insert(path.to_path_buf(), Instant::now());
            Ok(())
        } else {
            Err(Error::Watcher("Watcher not started".to_string()))
        }
    }

    pub fn unwatch(&self, path: &Path) -> Result<()> {
        let mut watcher = self.inner.lock();
        
        if let Some(w) = watcher.as_mut() {
            w.unwatch(path)?;
            self.watched_paths.lock().remove(path);
            Ok(())
        } else {
            Err(Error::Watcher("Watcher not started".to_string()))
        }
    }

    pub fn stop(&self) {
        *self.inner.lock() = None;
        self.watched_paths.lock().clear();
    }

    fn convert_event(
        event: Event,
        watched_paths: &Arc<Mutex<HashMap<PathBuf, Instant>>>,
        debounce_duration: Duration,
    ) -> Option<WatchEvent> {
        let now = Instant::now();
        let paths = event.paths;

        if paths.is_empty() {
            return None;
        }

        let path = &paths[0];
        
        {
            let mut cache = watched_paths.lock();
            if let Some(last_event) = cache.get(path) {
                if now.duration_since(*last_event) < debounce_duration {
                    return None;
                }
            }
            cache.insert(path.clone(), now);
        }

        match event.kind {
            EventKind::Create(_) => Some(WatchEvent::Created(path.clone())),
            
            EventKind::Modify(_) => Some(WatchEvent::Modified(path.clone())),
            
            EventKind::Remove(_) => Some(WatchEvent::Deleted(path.clone())),
            
            EventKind::Modify(notify::event::ModifyKind::Name(rename_mode)) => {
                use notify::event::RenameMode;
                match rename_mode {
                    RenameMode::Both => {
                        if paths.len() >= 2 {
                            Some(WatchEvent::Renamed {
                                from: paths[0].clone(),
                                to: paths[1].clone(),
                            })
                        } else {
                            None
                        }
                    }
                    RenameMode::From => Some(WatchEvent::Deleted(path.clone())),
                    RenameMode::To => Some(WatchEvent::Created(path.clone())),
                    _ => None,
                }
            }
            
            _ => None,
        }
    }

    pub fn is_watching(&self, path: &Path) -> bool {
        self.watched_paths.lock().contains_key(path)
    }

    pub fn watched_count(&self) -> usize {
        self.watched_paths.lock().len()
    }
}

impl Default for Watcher {
    fn default() -> Self {
        Self::new(DEBOUNCE_DURATION)
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_watch_create() {
        let temp_dir = TempDir::new().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        
        let watcher = Watcher::default();
        watcher.start(tx).unwrap();
        watcher.watch(temp_dir.path()).unwrap();

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        if let Some(event) = rx.try_recv().ok() {
            match event {
                WatchEvent::Created(path) => assert_eq!(path, test_file),
                _ => panic!("Expected Created event"),
            }
        }
    }

    #[tokio::test]
    async fn test_watch_delete() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        let (tx, mut rx) = mpsc::unbounded_channel();
        
        let watcher = Watcher::default();
        watcher.start(tx).unwrap();
        watcher.watch(temp_dir.path()).unwrap();

        fs::remove_file(&test_file).unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        if let Some(event) = rx.try_recv().ok() {
            match event {
                WatchEvent::Deleted(path) => assert_eq!(path, test_file),
                _ => panic!("Expected Deleted event"),
            }
        }
    }
}
