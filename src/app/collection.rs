use crate::download::{BeatmapStage, DownloadId, DownloadStage, DownloadSummary};
use ratatui::style::Color;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use crate::config::constants::{
    COMPLETION_PREFIXES, MAX_LOG_LINES, SPEED_STALE_AFTER, SPEED_UPDATE_INTERVAL,
};

/// minimum time between text updates on a single active-download slot.
const STATUS_DEBOUNCE: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Default)]
struct DisplayedStatus {
    message: String,
    rate_limited: bool,
}

fn non_empty_status(stage: BeatmapStage, beatmapset_id: u32, message: &str) -> String {
    if !message.trim().is_empty() {
        return message.to_string();
    }

    match stage {
        BeatmapStage::Pending => format!("queued #{beatmapset_id}"),
        BeatmapStage::Downloading => format!("downloading #{beatmapset_id}"),
        BeatmapStage::Verifying => format!("verifying #{beatmapset_id}"),
        BeatmapStage::Success => format!("done #{beatmapset_id}"),
        BeatmapStage::Skipped => format!("skipped #{beatmapset_id}"),
        BeatmapStage::Failed => format!("failed #{beatmapset_id}"),
        BeatmapStage::Aborted => format!("aborted #{beatmapset_id}"),
    }
}

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
    /// drives bar color and slot reuse; updated immediately on every status event so
    /// `first_free_slot` / `bar_color` see reality without lag.
    pub stage: BeatmapStage,
    pending: RefCell<DisplayedStatus>,
    displayed: RefCell<DisplayedStatus>,
    /// when `pending` is allowed to flip into `displayed`. `None` means `displayed` is current.
    debounce_at: Cell<Option<Instant>>,
    /// last time `displayed` was written. enforces a 50ms min gap between text updates.
    last_applied: Cell<Option<Instant>>,
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
            stage: BeatmapStage::Downloading,
            pending: RefCell::default(),
            displayed: RefCell::default(),
            debounce_at: Cell::new(None),
            last_applied: Cell::new(None),
            downloaded: 0,
            total: 0,
            bytes_for_speed: 0,
            last_update: None,
            speed_bytes_per_sec: 0.0,
        }
    }

    pub fn speed_bytes_per_sec(&self) -> f64 {
        self.last_update
            .filter(|last| last.elapsed() <= SPEED_STALE_AFTER)
            .map_or(0.0, |_| self.speed_bytes_per_sec)
    }

    pub fn is_completion_message(message: &str) -> bool {
        COMPLETION_PREFIXES.iter().any(|p| message.starts_with(p))
    }

    /// bar fill color for the current stage. rate_limited overrides downloading color.
    pub fn bar_color(&self) -> Color {
        use crate::tui::{ACCENT, DANGER, INFO, LINE_SOFT, SUCCESS, TEXT_DIM, TEXT_FAINT, WARNING};
        if matches!(self.stage, BeatmapStage::Downloading) && self.displayed_rate_limited() {
            return WARNING;
        }
        match self.stage {
            BeatmapStage::Pending => TEXT_FAINT,
            BeatmapStage::Downloading => ACCENT,
            BeatmapStage::Verifying => INFO,
            BeatmapStage::Success => SUCCESS,
            BeatmapStage::Skipped => LINE_SOFT,
            BeatmapStage::Failed => DANGER,
            BeatmapStage::Aborted => TEXT_DIM,
        }
    }

    fn apply_status(&mut self, stage: BeatmapStage, message: &str, rate_limited: bool) {
        // stage is structural (bar / slot reuse) and updates immediately. the *text* shown to
        // the user is rate-limited to one write per STATUS_DEBOUNCE for all stages — rapid
        // changes (download → verify → success in <50ms) coalesce to the last write.
        self.stage = stage;
        let next = DisplayedStatus {
            message: non_empty_status(stage, self.beatmapset_id, message),
            rate_limited,
        };
        *self.pending.borrow_mut() = next.clone();

        let now = Instant::now();
        let elapsed = self
            .last_applied
            .get()
            .map_or(Duration::MAX, |t| now.saturating_duration_since(t));
        if elapsed >= STATUS_DEBOUNCE {
            *self.displayed.borrow_mut() = next;
            self.last_applied.set(Some(now));
            self.debounce_at.set(None);
        } else {
            let last = self.last_applied.get().unwrap();
            self.debounce_at.set(Some(last + STATUS_DEBOUNCE));
        }
    }

    pub fn progress_ratio(&self) -> Option<f32> {
        (self.total > 0).then(|| (self.downloaded as f32 / self.total as f32).clamp(0.0, 1.0))
    }

    pub fn displayed_message(&self) -> String {
        self.resolve_pending();
        self.displayed.borrow().message.clone()
    }

    pub fn displayed_rate_limited(&self) -> bool {
        self.resolve_pending();
        self.displayed.borrow().rate_limited
    }

    fn resolve_pending(&self) {
        let Some(at) = self.debounce_at.get() else {
            return;
        };
        let now = Instant::now();
        if now >= at {
            *self.displayed.borrow_mut() = self.pending.borrow().clone();
            self.last_applied.set(Some(now));
            self.debounce_at.set(None);
        }
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
    /// Fixed-size slot list — one slot per worker thread. Free slots are `None`.
    /// Slot positions are stable for the lifetime of the page so completing
    /// downloads don't shift their neighbours up in the UI.
    pub active_downloads: Vec<Option<ActiveDownloadLine>>,
    pub concurrent: usize,
    index: HashMap<u32, usize>,
    pub logs: VecDeque<String>,
    pub summary: Option<DownloadSummary>,
    pub failed_maps: Vec<FailedBeatmap>,
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
        let slot_count = concurrent.max(1);
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
            active_downloads: (0..slot_count).map(|_| None).collect(),
            concurrent,
            index: HashMap::new(),
            logs: VecDeque::new(),
            summary: None,
            failed_maps: Vec::new(),
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

    pub fn active_lines(&self) -> impl Iterator<Item = &ActiveDownloadLine> {
        self.active_downloads
            .iter()
            .flatten()
            .filter(|line| !line.stage.is_terminal())
    }

    pub fn active_line_count(&self) -> usize {
        self.active_lines().count()
    }

    pub fn clear_active_downloads(&mut self) {
        self.active_downloads.fill(None);
    }

    pub fn all_active_rate_limited(&self) -> bool {
        let mut lines = self.active_lines().peekable();
        lines.peek().is_some() && lines.all(|l| l.displayed_rate_limited())
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
            if stage.is_terminal() {
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
        // terminal stages keep the line in place so the slot keeps rendering the final
        // message ("done via nerinyan") until another beatmapset reuses it — otherwise
        // the row would flash blank between completion and reuse
        if let Some(line) = self.find_active_line_mut(beatmapset_id) {
            line.apply_status(stage, message, rate_limited);
            return;
        }

        // Only an in-flight download stage may claim a free slot — precheck transitions
        // (`Pending`) update the underlying beatmap row but must not consume an active
        // slot, otherwise the panel grows past the worker count.
        if !matches!(stage, BeatmapStage::Downloading) {
            return;
        }
        let Some(slot_idx) = self.first_free_slot() else {
            return;
        };
        let mut line = ActiveDownloadLine::new(beatmapset_id);
        line.apply_status(stage, message, rate_limited);
        self.active_downloads[slot_idx] = Some(line);
    }

    pub fn update_active_progress(&mut self, beatmapset_id: u32, downloaded: u64, total: u64) {
        // Slot allocation is the status path's job — by the time progress arrives the lib
        // has already emitted `Contacting`/`Downloading` and the slot exists with a real
        // message. Allocating here would render a slot with an empty `displayed_message`.
        let Some(line) = self.find_active_line_mut(beatmapset_id) else {
            return;
        };
        line.downloaded = downloaded;
        if total > 0 {
            line.total = total;
        }
        let now = Instant::now();
        match line.last_update {
            Some(last) if now.duration_since(last) > SPEED_UPDATE_INTERVAL => {
                let elapsed = now.duration_since(last).as_secs_f64();
                let delta = downloaded.saturating_sub(line.bytes_for_speed) as f64;
                line.speed_bytes_per_sec = line.speed_bytes_per_sec * 0.7 + delta / elapsed * 0.3;
                line.bytes_for_speed = downloaded;
                line.last_update = Some(now);
            }
            None => {
                line.bytes_for_speed = downloaded;
                line.last_update = Some(now);
            }
            _ => {}
        }
    }

    fn find_active_line_mut(&mut self, beatmapset_id: u32) -> Option<&mut ActiveDownloadLine> {
        self.active_downloads
            .iter_mut()
            .flatten()
            .find(|line| line.beatmapset_id == beatmapset_id)
    }

    fn first_free_slot(&self) -> Option<usize> {
        // a terminal-stage slot counts as free — it's still rendered so the row isn't
        // blank, but a new beatmapset is welcome to overwrite it
        self.active_downloads
            .iter()
            .position(|slot| slot.as_ref().is_none_or(|line| line.stage.is_terminal()))
    }

    pub fn cumulative_speed(&self) -> f64 {
        let now = Instant::now();
        let stale = self
            .last_speed_update
            .get()
            .is_none_or(|last| now.duration_since(last) >= SPEED_UPDATE_INTERVAL);
        if stale {
            let speed = self.active_lines().map(|l| l.speed_bytes_per_sec()).sum();
            self.cached_cumulative_speed.set(speed);
            self.last_speed_update.set(Some(now));
        }
        self.cached_cumulative_speed.get()
    }

    pub fn set_failed_maps(&mut self, failures: Vec<(u32, String)>) {
        self.failed_maps = failures
            .into_iter()
            .map(|(id, reason)| FailedBeatmap { id, reason })
            .collect();
        self.failed_maps.sort_by_key(|f| f.id);
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
            return None;
        }
        let avg = self.stats.verify_total_us / self.stats.verify_total_count as u64;
        (avg > 0).then_some(avg)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/active_download_line.rs"]
mod tests;
