use crate::download::{BeatmapStage, DownloadId, DownloadStage, DownloadSummary, status};
use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use crate::config::constants::{
    COMPLETION_PREFIXES, MAX_LOG_LINES, SPEED_STALE_AFTER, SPEED_UPDATE_INTERVAL,
};

#[derive(Debug, Default, Clone)]
pub struct DownloadStats {
    pub downloaded: u32,
    pub skipped: u32,
    pub failed: u32,
    pub unverified: u32,
    pub bytes_downloaded: u64,
    pub total_collection_bytes: Option<u64>,
    pub verified_bytes: u64,
    pub verify_total_count: u32,
    pub verify_total_us: u64,
}

pub struct BeatmapRow {
    pub stage: BeatmapStage,
    pub message: String,
    pub progress: Option<(u64, u64)>,
}

#[derive(Debug, Clone)]
pub struct FailedBeatmap {
    pub id: u32,
    pub reason: String,
}

/// One line in the "active downloads" panel. Keyed by `beatmapset_id`.
#[derive(Debug, Clone)]
pub struct ActiveDownloadLine {
    pub beatmapset_id: u32,
    pub message: String,
    pub rate_limited: bool,
    pub downloaded: u64,
    pub total: u64,
    bytes_for_speed: u64,
    last_update: Option<Instant>,
    speed_bytes_per_sec: f64,
}

impl ActiveDownloadLine {
    fn new(beatmapset_id: u32) -> Self {
        Self {
            beatmapset_id,
            message: String::new(),
            rate_limited: false,
            downloaded: 0,
            total: 0,
            bytes_for_speed: 0,
            last_update: None,
            speed_bytes_per_sec: 0.0,
        }
    }

    pub fn speed_bytes_per_sec(&self) -> f64 {
        match self.last_update {
            Some(last) if last.elapsed() <= SPEED_STALE_AFTER => self.speed_bytes_per_sec,
            _ => 0.0,
        }
    }

    pub fn is_completion_message(message: &str) -> bool {
        COMPLETION_PREFIXES
            .iter()
            .any(|prefix| message.starts_with(prefix))
    }

    pub fn should_show_bar(&self) -> bool {
        self.message.starts_with(status::DOWNLOADING) && self.total > 0
    }

    pub fn progress_ratio(&self) -> Option<f32> {
        if self.total == 0 {
            return None;
        }
        let ratio = self.downloaded as f32 / self.total as f32;
        Some(ratio.clamp(0.0, 1.0))
    }
}

pub struct CollectionPage {
    pub id: DownloadId,
    pub title: String,
    pub stage: DownloadStage,
    pub total_maps: usize,
    pub download_target: usize,
    pub stats: DownloadStats,
    pub output_dir: Option<String>,
    pub uploader: Option<String>,
    pub beatmaps: Vec<BeatmapRow>,
    pub active_downloads: Vec<ActiveDownloadLine>,
    pub concurrent: usize,
    index: HashMap<u32, usize>,
    pub logs: VecDeque<String>,
    pub summary: Option<DownloadSummary>,
    pub failed_maps: Vec<FailedBeatmap>,
    pub progress_label_style_locked: bool,
    pub progress_label_bold_when_locked: bool,
    pub low_disk_space: Option<u64>,
    pub resolve_progress: Option<(u32, u32)>,
    pub thread_scroll: usize,
    pub thread_visible_items: Cell<usize>,
    pub thread_total_items: Cell<usize>,
    pub indeterminate_anim_start: Cell<Option<u64>>,
    cached_cumulative_speed: Cell<f64>,
    last_speed_update: Cell<Option<Instant>>,
}

impl CollectionPage {
    pub fn new(id: DownloadId, title: String, concurrent: usize) -> Self {
        Self {
            id,
            title,
            stage: DownloadStage::Pending,
            total_maps: 0,
            download_target: 0,
            stats: DownloadStats::default(),
            output_dir: None,
            uploader: None,
            beatmaps: Vec::new(),
            active_downloads: Vec::new(),
            concurrent,
            index: HashMap::new(),
            logs: VecDeque::new(),
            summary: None,
            failed_maps: Vec::new(),
            progress_label_style_locked: false,
            progress_label_bold_when_locked: true,
            low_disk_space: None,
            resolve_progress: None,
            thread_scroll: 0,
            thread_visible_items: Cell::new(0),
            thread_total_items: Cell::new(0),
            indeterminate_anim_start: Cell::new(None),
            cached_cumulative_speed: Cell::new(0.0),
            last_speed_update: Cell::new(None),
        }
    }

    pub fn all_active_rate_limited(&self) -> bool {
        !self.active_downloads.is_empty()
            && self.active_downloads.iter().all(|line| line.rate_limited)
    }

    pub fn register_beatmaps(&mut self, ids: &[u32]) {
        self.beatmaps.clear();
        self.index.clear();
        self.failed_maps.clear();
        for (idx, beatmapset_id) in ids.iter().copied().enumerate() {
            self.index.insert(beatmapset_id, idx);
            self.beatmaps.push(BeatmapRow {
                stage: BeatmapStage::Pending,
                message: String::from("waiting"),
                progress: None,
            });
        }
    }

    pub fn update_progress(&mut self, beatmapset_id: u32, downloaded: u64, total: u64) {
        if let Some(idx) = self.index.get(&beatmapset_id).copied()
            && let Some(row) = self.beatmaps.get_mut(idx)
        {
            if let Some((prev_downloaded, _)) = row.progress {
                self.stats.bytes_downloaded =
                    self.stats.bytes_downloaded.saturating_sub(prev_downloaded) + downloaded;
            } else {
                self.stats.bytes_downloaded += downloaded;
            }
            row.progress = Some((downloaded, total));
        }
    }

    pub fn update_status(&mut self, beatmapset_id: u32, stage: BeatmapStage, message: &str) {
        if let Some(idx) = self.index.get(&beatmapset_id).copied()
            && let Some(row) = self.beatmaps.get_mut(idx)
        {
            row.stage = stage;
            row.message = message.to_string();
            if matches!(
                stage,
                BeatmapStage::Success
                    | BeatmapStage::Skipped
                    | BeatmapStage::Failed
                    | BeatmapStage::Aborted
            ) {
                row.progress = None;
            }
        }
    }

    pub fn push_log(&mut self, message: &str) {
        self.logs.push_front(message.to_string());
        while self.logs.len() > MAX_LOG_LINES {
            self.logs.pop_back();
        }
    }

    pub fn update_active_status(
        &mut self,
        beatmapset_id: u32,
        stage: BeatmapStage,
        message: &str,
        rate_limited: bool,
    ) {
        let terminal = matches!(
            stage,
            BeatmapStage::Success
                | BeatmapStage::Skipped
                | BeatmapStage::Failed
                | BeatmapStage::Aborted
        );

        if terminal {
            self.active_downloads
                .retain(|line| line.beatmapset_id != beatmapset_id);
            return;
        }

        let line = self.active_or_insert(beatmapset_id);
        line.message = message.to_string();
        line.rate_limited = rate_limited;
    }

    pub fn update_active_progress(&mut self, beatmapset_id: u32, downloaded: u64, total: u64) {
        let line = self.active_or_insert(beatmapset_id);
        line.downloaded = downloaded;
        if total > 0 {
            line.total = total;
        }
        let now = Instant::now();
        match line.last_update {
            Some(last) => {
                let elapsed = now.duration_since(last);
                if elapsed > SPEED_UPDATE_INTERVAL {
                    let delta = downloaded.saturating_sub(line.bytes_for_speed);
                    let speed = delta as f64 / elapsed.as_secs_f64();
                    line.speed_bytes_per_sec = line.speed_bytes_per_sec * 0.7 + speed * 0.3;
                    line.bytes_for_speed = downloaded;
                    line.last_update = Some(now);
                }
            }
            None => {
                line.bytes_for_speed = downloaded;
                line.last_update = Some(now);
            }
        }
    }

    fn active_or_insert(&mut self, beatmapset_id: u32) -> &mut ActiveDownloadLine {
        if let Some(idx) = self
            .active_downloads
            .iter()
            .position(|line| line.beatmapset_id == beatmapset_id)
        {
            return &mut self.active_downloads[idx];
        }
        self.active_downloads
            .push(ActiveDownloadLine::new(beatmapset_id));
        self.active_downloads.last_mut().unwrap()
    }

    pub fn cumulative_speed(&self) -> f64 {
        let now = Instant::now();
        let should_update = match self.last_speed_update.get() {
            Some(last) => now.duration_since(last) >= SPEED_UPDATE_INTERVAL,
            None => true,
        };

        if should_update {
            let new_speed: f64 = self
                .active_downloads
                .iter()
                .map(|line| line.speed_bytes_per_sec())
                .sum();
            self.cached_cumulative_speed.set(new_speed);
            self.last_speed_update.set(Some(now));
        }

        self.cached_cumulative_speed.get()
    }

    pub fn set_failed_maps(&mut self, failures: Vec<(u32, String)>) {
        let mut entries: Vec<FailedBeatmap> = failures
            .into_iter()
            .map(|(id, reason)| FailedBeatmap { id, reason })
            .collect();
        entries.sort_by_key(|a| a.id);
        self.failed_maps = entries;
    }

    pub fn scroll_threads_up(&mut self) {
        self.thread_scroll = self.thread_scroll.saturating_sub(1);
    }

    pub fn scroll_threads_down(&mut self) {
        let total = self.thread_total_items.get();
        let visible = self.thread_visible_items.get();
        let max_scroll = total.saturating_sub(visible);
        if self.thread_scroll < max_scroll {
            self.thread_scroll += 1;
        }
    }

    pub fn total_downloaded_bytes(&self) -> u64 {
        self.stats
            .bytes_downloaded
            .saturating_add(self.stats.verified_bytes)
    }

    pub fn avg_verify_us(&self) -> Option<u64> {
        if self.stats.verify_total_count == 0 {
            None
        } else {
            Some(self.stats.verify_total_us / self.stats.verify_total_count as u64)
        }
    }
}

#[allow(dead_code)]
const STATUS_DEBOUNCE: Duration = Duration::from_millis(100);
