use dashmap::DashSet;
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;

#[derive(Clone, Default)]
pub struct CleanupTracker {
    pending: Arc<DashSet<PathBuf>>,
}

pub struct CleanupOutcome {
    pub removed: usize,
    pub failures: Vec<(PathBuf, String)>,
}

impl CleanupTracker {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(DashSet::new()),
        }
    }

    pub fn track(&self, path: &Path) {
        self.pending.insert(path.to_path_buf());
    }

    pub fn forget(&self, path: &Path) {
        self.pending.remove(path);
    }

    pub async fn cleanup_incomplete(&self) -> CleanupOutcome {
        let paths: Vec<PathBuf> = self.pending.iter().map(|r| r.key().clone()).collect();
        let mut removed = 0;
        let mut failures = Vec::new();

        for path in paths {
            match fs::remove_file(&path).await {
                Ok(_) => {
                    removed += 1;
                    self.pending.remove(&path);
                }
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    self.pending.remove(&path);
                }
                Err(err) => failures.push((path, err.to_string())),
            }
        }

        CleanupOutcome { removed, failures }
    }
}
