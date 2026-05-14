use crate::{config::constants::DIRECTORY_LOCK_FILE, download::error::DownloadError};
use dashmap::DashSet;
use fs2::FileExt;
use std::{
    fs::{File as StdFile, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::warn;

#[derive(Clone)]
pub struct ActiveDownloadRegistry {
    active: Arc<DashSet<PathBuf>>,
}

impl ActiveDownloadRegistry {
    pub fn new() -> Self {
        Self {
            active: Arc::new(DashSet::new()),
        }
    }

    pub fn try_insert(&self, path: &Path) -> bool {
        self.active.insert(path.to_path_buf())
    }

    pub fn remove(&self, path: &Path) {
        self.active.remove(path);
    }
}

impl Default for ActiveDownloadRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DownloadLockGuard {
    path: PathBuf,
    file: Option<StdFile>,
    registry: ActiveDownloadRegistry,
}

impl DownloadLockGuard {
    pub fn acquire(path: &Path, registry: &ActiveDownloadRegistry) -> Result<Self, DownloadError> {
        if !registry.try_insert(path) {
            return Err(DownloadError::ConcurrentDownload(
                path.to_string_lossy().into_owned(),
            ));
        }

        match Self::lock_directory(path) {
            Ok(file) => Ok(Self {
                path: path.to_path_buf(),
                file: Some(file),
                registry: registry.clone(),
            }),
            Err(err) => {
                registry.remove(path);
                Err(err)
            }
        }
    }

    fn lock_directory(path: &Path) -> Result<StdFile, DownloadError> {
        let lock_file_path = path.join(DIRECTORY_LOCK_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)
            .map_err(DownloadError::from)?;

        if let Err(err) = file.try_lock_exclusive() {
            let kind = err.kind();
            drop(file);
            if kind == io::ErrorKind::WouldBlock {
                return Err(DownloadError::ConcurrentDownload(
                    path.to_string_lossy().into_owned(),
                ));
            }
            return Err(DownloadError::Io(err));
        }

        Ok(file)
    }
}

impl Drop for DownloadLockGuard {
    fn drop(&mut self) {
        if let Some(file) = self.file.take()
            && let Err(err) = file.unlock()
        {
            warn!(
                directory = %self.path.display(),
                error = %err,
                "Failed to release directory lock"
            );
        }

        // Leave the lock file in place. Removing it between unlock and the next
        // process acquiring it creates a race where two processes both see a fresh
        // file and both believe they hold the lock.
        self.registry.remove(&self.path);
    }
}
