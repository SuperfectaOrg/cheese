use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::fs;
use std::time::SystemTime;
use chrono::{DateTime, Utc};
use xdg::BaseDirectories;

pub struct Trash {
    trash_dir: PathBuf,
    files_dir: PathBuf,
    info_dir: PathBuf,
}

impl Trash {
    pub fn new() -> Result<Self> {
        let xdg_dirs = BaseDirectories::new()
            .map_err(|e| Error::TrashError(format!("Failed to get XDG directories: {}", e)))?;
        
        let trash_dir = xdg_dirs.get_data_home().join("Trash");
        let files_dir = trash_dir.join("files");
        let info_dir = trash_dir.join("info");

        fs::create_dir_all(&files_dir)?;
        fs::create_dir_all(&info_dir)?;

        Ok(Self {
            trash_dir,
            files_dir,
            info_dir,
        })
    }

    pub fn send_to_trash(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(Error::NotFound { path: path.to_path_buf() });
        }

        let file_name = path.file_name()
            .ok_or_else(|| Error::InvalidPath { path: path.to_path_buf() })?
            .to_string_lossy()
            .to_string();

        let unique_name = self.find_unique_trash_name(&file_name)?;
        let trash_file_path = self.files_dir.join(&unique_name);
        let trash_info_path = self.info_dir.join(format!("{}.trashinfo", unique_name));

        let original_path = path.canonicalize()?;
        let deletion_date = SystemTime::now();

        self.create_trash_info(&trash_info_path, &original_path, deletion_date)?;

        fs::rename(path, &trash_file_path).map_err(|e| {
            let _ = fs::remove_file(&trash_info_path);
            Error::TrashError(format!("Failed to move file to trash: {}", e))
        })?;

        Ok(())
    }

    pub fn restore(&self, trash_name: &str) -> Result<PathBuf> {
        let trash_file_path = self.files_dir.join(trash_name);
        let trash_info_path = self.info_dir.join(format!("{}.trashinfo", trash_name));

        if !trash_file_path.exists() {
            return Err(Error::NotFound { path: trash_file_path });
        }

        let original_path = self.read_trash_info(&trash_info_path)?;

        if original_path.exists() {
            return Err(Error::AlreadyExists { path: original_path });
        }

        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::rename(&trash_file_path, &original_path)?;
        fs::remove_file(&trash_info_path)?;

        Ok(original_path)
    }

    pub fn empty_trash(&self) -> Result<()> {
        for entry in fs::read_dir(&self.files_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }

        for entry in fs::read_dir(&self.info_dir)? {
            let entry = entry?;
            fs::remove_file(entry.path())?;
        }

        Ok(())
    }

    pub fn list_trash_items(&self) -> Result<Vec<TrashItem>> {
        let mut items = Vec::new();

        for entry in fs::read_dir(&self.info_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("trashinfo") {
                continue;
            }

            let trash_name = path.file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| Error::TrashError("Invalid trash info file".to_string()))?
                .to_string();

            let original_path = self.read_trash_info(&path)?;
            let deletion_date = self.read_deletion_date(&path)?;
            let trash_file_path = self.files_dir.join(&trash_name);

            let size = if trash_file_path.exists() {
                self.get_size_recursive(&trash_file_path)?
            } else {
                0
            };

            items.push(TrashItem {
                trash_name,
                original_path,
                deletion_date,
                size,
            });
        }

        Ok(items)
    }

    pub fn permanently_delete(&self, trash_name: &str) -> Result<()> {
        let trash_file_path = self.files_dir.join(trash_name);
        let trash_info_path = self.info_dir.join(format!("{}.trashinfo", trash_name));

        if trash_file_path.exists() {
            if trash_file_path.is_dir() {
                fs::remove_dir_all(&trash_file_path)?;
            } else {
                fs::remove_file(&trash_file_path)?;
            }
        }

        if trash_info_path.exists() {
            fs::remove_file(&trash_info_path)?;
        }

        Ok(())
    }

    fn create_trash_info(&self, info_path: &Path, original_path: &Path, deletion_date: SystemTime) -> Result<()> {
        let datetime: DateTime<Utc> = deletion_date.into();
        let formatted_date = datetime.format("%Y-%m-%dT%H:%M:%S").to_string();

        let content = format!(
            "[Trash Info]\nPath={}\nDeletionDate={}\n",
            original_path.display(),
            formatted_date
        );

        fs::write(info_path, content)?;
        Ok(())
    }

    fn read_trash_info(&self, info_path: &Path) -> Result<PathBuf> {
        let content = fs::read_to_string(info_path)?;

        for line in content.lines() {
            if let Some(path_str) = line.strip_prefix("Path=") {
                return Ok(PathBuf::from(path_str));
            }
        }

        Err(Error::TrashError("Invalid trash info format".to_string()))
    }

    fn read_deletion_date(&self, info_path: &Path) -> Result<SystemTime> {
        let content = fs::read_to_string(info_path)?;

        for line in content.lines() {
            if let Some(date_str) = line.strip_prefix("DeletionDate=") {
                let datetime = DateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S")
                    .or_else(|_| DateTime::parse_from_rfc3339(date_str))
                    .map_err(|e| Error::TrashError(format!("Invalid date format: {}", e)))?;
                return Ok(datetime.into());
            }
        }

        Ok(SystemTime::now())
    }

    fn find_unique_trash_name(&self, base_name: &str) -> Result<String> {
        let mut name = base_name.to_string();
        let mut counter = 1;

        while self.files_dir.join(&name).exists() {
            let (stem, ext) = if let Some(dot_pos) = base_name.rfind('.') {
                (&base_name[..dot_pos], &base_name[dot_pos..])
            } else {
                (base_name, "")
            };

            name = format!("{}.{}{}", stem, counter, ext);
            counter += 1;

            if counter > 9999 {
                return Err(Error::TrashError("Too many files with same name in trash".to_string()));
            }
        }

        Ok(name)
    }

    fn get_size_recursive(&self, path: &Path) -> Result<u64> {
        let metadata = fs::metadata(path)?;

        if metadata.is_file() {
            return Ok(metadata.len());
        }

        let mut total = 0u64;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            total += self.get_size_recursive(&entry.path())?;
        }

        Ok(total)
    }

    pub fn trash_size(&self) -> Result<u64> {
        self.get_size_recursive(&self.files_dir)
    }
}

impl Default for Trash {
    fn default() -> Self {
        Self::new().expect("Failed to initialize trash")
    }
}

#[derive(Debug, Clone)]
pub struct TrashItem {
    pub trash_name: String,
    pub original_path: PathBuf,
    pub deletion_date: SystemTime,
    pub size: u64,
}
