use crate::config::constants::{DISK_DANGER_BYTES, DISK_WARN_BYTES};

/// A persistent, condition-driven banner shown at the top of a tab.
///
/// Banners are derived from shared `App` state each frame and auto-clear
/// when the underlying condition resolves. They are NOT stored in `App` —
/// they are computed on demand from existing fields.
///
/// Display order when multiple are active: `DiskFull` > `DiskLow`
/// (most critical first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Banner {
    /// Disk is dangerously low (below [`DISK_DANGER_BYTES`], 100 MiB).
    DiskFull { free_bytes: u64 },
    /// Disk is low (below [`DISK_WARN_BYTES`], 1 GiB).
    DiskLow { free_bytes: u64 },
}

/// Compute the list of banners that should be shown on the home tab.
///
/// Returns an empty `Vec` when no banners are active.
pub fn home_banners(disk_free: Option<u64>) -> Vec<Banner> {
    let Some(free) = disk_free else {
        return Vec::new();
    };

    if free < DISK_DANGER_BYTES {
        vec![Banner::DiskFull { free_bytes: free }]
    } else if free < DISK_WARN_BYTES {
        vec![Banner::DiskLow { free_bytes: free }]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/banner.rs"]
mod tests;
