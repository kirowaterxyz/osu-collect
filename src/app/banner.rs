use std::cell::Cell;

use crate::config::constants::{DISK_DANGER_BYTES, DISK_WARN_BYTES};
use crate::tui::COMPACT_HEIGHT;

/// A persistent, condition-driven, system-wide banner shown at the top of the
/// body on every tab.
///
/// Banners are derived from shared `App` state each frame and auto-clear
/// when the underlying condition resolves. They are NOT stored in `App` —
/// they are computed on demand from existing fields.
///
/// At most one banner shows at a time. `DiskFull` (DANGER) always wins; among
/// the WARNING conditions (`DiskLow`, `TooSmall`) the tie is broken by the
/// most-recently-entered condition (see [`BannerRecency`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Banner {
    /// Disk is dangerously low (below [`DISK_DANGER_BYTES`], 100 MiB).
    DiskFull { free_bytes: u64 },
    /// Disk is low (below [`DISK_WARN_BYTES`], 1 GiB).
    DiskLow { free_bytes: u64 },
    /// Terminal body is below the compact threshold — the full layout shrinks.
    TooSmall,
}

/// Per-WARNING-condition entry timestamps, used to break banner ties by which
/// condition was entered most recently (cloudy-tui banner winner-ordering:
/// DANGER > WARNING, WARNING ties broken by most-recently-entered).
///
/// Held by `App` and updated under an immutable borrow during `draw` (interior
/// mutability mirrors `disk_cache` / `help_scroll`). Each `Cell` holds the tick
/// at which its condition last became active, or `None` while inactive.
#[derive(Debug, Default)]
pub struct BannerRecency {
    disk_low_since: Cell<Option<u64>>,
    too_small_since: Cell<Option<u64>>,
}

impl BannerRecency {
    /// Record `active` for one condition's `Cell`, stamping `tick` on the
    /// inactive→active edge and clearing it when the condition resolves.
    /// Returns the entry tick while active (`None` when inactive).
    fn track(cell: &Cell<Option<u64>>, active: bool, tick: u64) -> Option<u64> {
        if !active {
            cell.set(None);
            return None;
        }
        let since = cell.get().unwrap_or(tick);
        cell.set(Some(since));
        Some(since)
    }
}

/// Compute the single system-wide banner to show, if any.
///
/// At most one banner. `DiskFull` (DANGER) outranks every WARNING. `disk_free`
/// is the free space on the active output filesystem; `content_height` is the
/// pre-split body area height (before any banner row is carved off). The views
/// strip chrome on their POST-split height, which is one row shorter whenever a
/// banner is shown — so `TooSmall` is decided against that same post-split
/// height: when a disk banner already occupies the row, the body the views
/// receive is `content_height - 1`, and `TooSmall` surfaces below
/// [`COMPACT_HEIGHT`] there too. This keeps the `TooSmall` cue and the views'
/// chrome-stripping in agreement (no compact layout without a banner cue).
/// When both WARNING conditions (`DiskLow`, `TooSmall`) are live the tie goes to
/// whichever was entered most recently, tracked across frames via `recency` and
/// the monotonic `tick`. Returns an empty `Vec` when no condition is active.
pub fn system_banners(
    disk_free: Option<u64>,
    content_height: u16,
    recency: &BannerRecency,
    tick: u64,
) -> Vec<Banner> {
    if let Some(free) = disk_free
        && free < DISK_DANGER_BYTES
    {
        // DANGER short-circuits; clear WARNING entry stamps so a later
        // inactive→active edge re-stamps fresh.
        BannerRecency::track(&recency.disk_low_since, false, tick);
        BannerRecency::track(&recency.too_small_since, false, tick);
        return vec![Banner::DiskFull { free_bytes: free }];
    }

    let disk_low = disk_free.filter(|&b| b < DISK_WARN_BYTES);
    // A disk banner steals one body row; the views then strip chrome on that
    // shorter height, so compare `TooSmall` against the post-split body height.
    let body_height = if disk_low.is_some() {
        content_height.saturating_sub(1)
    } else {
        content_height
    };
    let disk_low_since = BannerRecency::track(&recency.disk_low_since, disk_low.is_some(), tick);
    let too_small_since =
        BannerRecency::track(&recency.too_small_since, body_height < COMPACT_HEIGHT, tick);

    match (disk_low, too_small_since) {
        // Both WARNING conditions live: most-recently-entered wins.
        (Some(free), Some(small_since)) => {
            let low_since = disk_low_since.unwrap_or(tick);
            if small_since > low_since {
                vec![Banner::TooSmall]
            } else {
                vec![Banner::DiskLow { free_bytes: free }]
            }
        }
        (Some(free), None) => vec![Banner::DiskLow { free_bytes: free }],
        (None, Some(_)) => vec![Banner::TooSmall],
        (None, None) => Vec::new(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/banner.rs"]
mod tests;
