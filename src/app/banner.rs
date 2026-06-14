use crate::config::constants::{DISK_DANGER_BYTES, DISK_WARN_BYTES};
use crate::tui::COMPACT_HEIGHT;

/// A persistent, condition-driven, system-wide banner shown at the top of the
/// body on every tab.
///
/// Banners are derived from shared `App` state each frame and auto-clear
/// when the underlying condition resolves. They are NOT stored in `App` —
/// they are computed on demand from existing fields.
///
/// At most one banner shows at a time (highest semantic severity wins):
/// `DiskFull` (DANGER) > `DiskLow` / `TooSmall` (WARNING).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Banner {
    /// Disk is dangerously low (below [`DISK_DANGER_BYTES`], 100 MiB).
    DiskFull { free_bytes: u64 },
    /// Disk is low (below [`DISK_WARN_BYTES`], 1 GiB).
    DiskLow { free_bytes: u64 },
    /// Terminal body is below the compact threshold — the full layout shrinks.
    TooSmall,
}

/// Compute the single system-wide banner to show, if any.
///
/// At most one banner (cloudy-tui: highest semantic severity wins). `disk_free`
/// is the free space on the active output filesystem; `content_height` is the
/// body area height. Below [`COMPACT_HEIGHT`] a "terminal too small" WARNING
/// banner surfaces; disk conditions (which carry an action hint) outrank it.
/// Returns an empty `Vec` when no condition is active.
pub fn system_banners(disk_free: Option<u64>, content_height: u16) -> Vec<Banner> {
    if let Some(free) = disk_free {
        if free < DISK_DANGER_BYTES {
            return vec![Banner::DiskFull { free_bytes: free }];
        }
        if free < DISK_WARN_BYTES {
            return vec![Banner::DiskLow { free_bytes: free }];
        }
    }
    if content_height < COMPACT_HEIGHT {
        return vec![Banner::TooSmall];
    }
    Vec::new()
}

#[cfg(test)]
#[path = "../../tests/unit/banner.rs"]
mod tests;
