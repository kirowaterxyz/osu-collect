use super::{BeatmapStage, DownloadError, DownloadEvent, DownloadId};
use crate::config::constants::VALIDATION_CACHE_LIMIT;
use dashmap::DashMap;
use futures_util::{StreamExt, stream};
use osu_downloader::{
    ArchiveValidation, ArchiveValidationResult, OutputEntry, classify_output_entry,
    validate_and_remove,
};

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Instant,
};
use tokio::{fs, sync::watch};
use tracing::{debug, info, warn};

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub(crate) enum CacheKey {
    #[cfg(unix)]
    FileId { device: u64, inode: u64, size: u64 },
    #[cfg(not(unix))]
    Path {
        path: PathBuf,
        size: u64,
        mtime: Option<SystemTime>,
    },
}

impl CacheKey {
    pub fn from_meta(_path: &Path, meta: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            CacheKey::FileId {
                device: meta.dev(),
                inode: meta.ino(),
                size: meta.len(),
            }
        }
        #[cfg(not(unix))]
        {
            CacheKey::Path {
                path: _path.to_path_buf(),
                size: meta.len(),
                mtime: meta.modified().ok(),
            }
        }
    }

    fn size(&self) -> u64 {
        match self {
            #[cfg(unix)]
            CacheKey::FileId { size, .. } => *size,
            #[cfg(not(unix))]
            CacheKey::Path { size, .. } => *size,
        }
    }
}

/// In-memory cache of previously-validated `.osz` archives. Each entry records
/// the strictest [`ArchiveValidation`] mode the file passed; a lookup at
/// strictness `requested` is only a hit when the stored mode is `>= requested`.
/// `Off` is never cached — it skips ZIP-shape validation, so caching it would
/// satisfy stricter probes without actually checking.
#[derive(Default)]
pub(crate) struct ValidationCache {
    entries: DashMap<CacheKey, ArchiveValidation>,
}

impl ValidationCache {
    pub fn is_valid(&self, key: &CacheKey, requested: ArchiveValidation) -> bool {
        self.entries
            .get(key)
            .is_some_and(|stored| *stored >= requested)
    }

    pub fn mark_valid(&self, key: CacheKey, mode: ArchiveValidation) {
        if mode == ArchiveValidation::Off {
            return;
        }
        if self.entries.len() >= VALIDATION_CACHE_LIMIT {
            self.entries.clear();
        }
        self.entries
            .entry(key)
            .and_modify(|existing| {
                if mode > *existing {
                    *existing = mode;
                }
            })
            .or_insert(mode);
    }
}

static VALIDATION_CACHE: LazyLock<ValidationCache> = LazyLock::new(ValidationCache::default);

pub(crate) struct PrecheckReport {
    pub(crate) satisfied: HashSet<u32>,
    pub(crate) skipped: u32,
    pub(crate) unverified: Vec<u32>,
    pub(crate) verified_bytes: u64,
    pub(crate) aborted: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PrecheckOptions {
    pub(crate) notify_verified: bool,
    pub(crate) archive_validation: ArchiveValidation,
}

pub(crate) async fn verify_existing_beatmapsets(
    id: DownloadId,
    output_dir: &Path,
    expectations: Arc<HashSet<u32>>,
    parallelism: usize,
    options: PrecheckOptions,
    cancel_rx: &watch::Receiver<bool>,
    emit: impl Fn(DownloadEvent) + Send + Sync,
) -> Result<PrecheckReport, DownloadError> {
    info!(
        download_id = id,
        directory = %output_dir.display(),
        "starting existing file verification"
    );

    if *cancel_rx.borrow() {
        return Ok(PrecheckReport {
            satisfied: HashSet::new(),
            skipped: 0,
            unverified: Vec::new(),
            verified_bytes: 0,
            aborted: true,
        });
    }

    let mut state = PrecheckState::default();
    let CandidateScan {
        candidates,
        orphan_temp_count,
        aborted,
    } = scan_candidates(output_dir, &expectations, cancel_rx).await?;
    if aborted {
        return Ok(state.aborted_report());
    }

    let worker_count = parallelism.max(1);
    let mut tasks = stream::iter(candidates)
        .map(|candidate| validate_existing_candidate(candidate, options, cancel_rx.clone()))
        .buffer_unordered(worker_count);

    let mut aborted = false;
    while let Some(result) = tasks.next().await {
        if *cancel_rx.borrow() {
            aborted = true;
            break;
        }
        match result {
            Ok(Some(record)) => state.record(id, record, options, &emit),
            Ok(None) => {}
            Err((path, error)) => {
                warn!(file = %path.display(), error = %error, "failed to process existing file");
            }
        }
    }

    if aborted {
        info!(
            download_id = id,
            verified = state.satisfied.len(),
            "existing file verification aborted by shutdown"
        );
    } else {
        info!(
            download_id = id,
            verified = state.satisfied.len(),
            skipped = state.skipped,
            unverified = state.unverified.len(),
            orphan_temp = orphan_temp_count,
            "existing file verification complete"
        );
        if orphan_temp_count > 0 {
            emit(DownloadEvent::Log {
                id,
                message: format!("removed {orphan_temp_count} orphaned temp download file(s)"),
            });
        }
    }

    Ok(PrecheckReport {
        satisfied: state.satisfied,
        skipped: state.skipped,
        unverified: state.unverified,
        verified_bytes: state.verified_bytes,
        aborted,
    })
}

#[derive(Default)]
struct PrecheckState {
    satisfied: HashSet<u32>,
    skipped: u32,
    unverified: Vec<u32>,
    verified_bytes: u64,
    unverified_maps: HashSet<u32>,
}

impl PrecheckState {
    fn aborted_report(&self) -> PrecheckReport {
        PrecheckReport {
            satisfied: self.satisfied.clone(),
            skipped: self.skipped,
            unverified: Vec::new(),
            verified_bytes: self.verified_bytes,
            aborted: true,
        }
    }

    fn record(
        &mut self,
        id: DownloadId,
        mut record: FileRecord,
        options: PrecheckOptions,
        emit: &impl Fn(DownloadEvent),
    ) {
        if let Some(error) = record.validation_error.take() {
            self.record_invalid(id, record, error, options, emit);
            return;
        }

        if !self.satisfied.insert(record.beatmapset_id) {
            return;
        }

        self.skipped = self.skipped.saturating_add(1);
        self.verified_bytes += record.file_size;

        if options.notify_verified {
            emit(DownloadEvent::BeatmapStatus {
                id,
                beatmapset_id: record.beatmapset_id,
                stage: BeatmapStage::Skipped,
                message: "already present".to_string(),
                rate_limited: false,
            });
            emit(DownloadEvent::BeatmapVerified {
                id,
                duration_us: record.duration_us,
            });
            self.emit_progress(id, emit);
        }
    }

    fn record_invalid(
        &mut self,
        id: DownloadId,
        record: FileRecord,
        error: String,
        options: PrecheckOptions,
        emit: &impl Fn(DownloadEvent),
    ) {
        if self.unverified_maps.insert(record.beatmapset_id) {
            self.unverified.push(record.beatmapset_id);
        }
        warn!(
            download_id = id,
            beatmapset_id = record.beatmapset_id,
            file = %record.path.display(),
            error = %error,
            "existing archive failed validation"
        );
        if options.notify_verified {
            emit(DownloadEvent::BeatmapStatus {
                id,
                beatmapset_id: record.beatmapset_id,
                stage: BeatmapStage::Failed,
                message: format!("existing file failed validation: {error}"),
                rate_limited: false,
            });
            emit(DownloadEvent::BeatmapVerified {
                id,
                duration_us: record.duration_us,
            });
            self.emit_progress(id, emit);
        }
    }

    fn emit_progress(&self, id: DownloadId, emit: &impl Fn(DownloadEvent)) {
        emit(DownloadEvent::OverallProgress {
            id,
            downloaded: 0,
            skipped: self.skipped,
            failed: 0,
            unverified: self.unverified.len() as u32,
        });
    }
}

#[derive(Clone)]
struct Candidate {
    path: PathBuf,
    beatmapset_id: u32,
}

struct CandidateScan {
    candidates: Vec<Candidate>,
    orphan_temp_count: usize,
    aborted: bool,
}

#[derive(Debug)]
struct FileRecord {
    beatmapset_id: u32,
    file_size: u64,
    path: PathBuf,
    validation_error: Option<String>,
    duration_us: u64,
}

async fn scan_candidates(
    output_dir: &Path,
    expectations: &HashSet<u32>,
    cancel_rx: &watch::Receiver<bool>,
) -> Result<CandidateScan, DownloadError> {
    let mut dir = fs::read_dir(output_dir).await?;
    let mut candidates = Vec::new();
    let mut orphan_temp_count = 0usize;

    while let Some(entry) = dir.next_entry().await? {
        if *cancel_rx.borrow() {
            return Ok(CandidateScan {
                candidates: Vec::new(),
                orphan_temp_count,
                aborted: true,
            });
        }

        match classify_output_entry(&entry.file_name()) {
            OutputEntry::OrphanTemp => {
                orphan_temp_count += remove_orphan_temp(entry.path()).await as usize;
            }
            OutputEntry::Archive { beatmapset_id } if expectations.contains(&beatmapset_id) => {
                candidates.push(Candidate {
                    path: entry.path(),
                    beatmapset_id,
                });
            }
            _ => {}
        }
    }

    if *cancel_rx.borrow() {
        return Ok(CandidateScan {
            candidates: Vec::new(),
            orphan_temp_count,
            aborted: true,
        });
    }

    Ok(CandidateScan {
        candidates,
        orphan_temp_count,
        aborted: false,
    })
}

async fn remove_orphan_temp(path: PathBuf) -> bool {
    match fs::remove_file(&path).await {
        Ok(()) => {
            debug!(file = %path.display(), "removed orphaned temp download file");
            true
        }
        Err(err) => {
            warn!(file = %path.display(), error = %err, "failed to remove orphaned temp download file");
            false
        }
    }
}

async fn validate_existing_candidate(
    candidate: Candidate,
    options: PrecheckOptions,
    cancel_rx: watch::Receiver<bool>,
) -> Result<Option<FileRecord>, (PathBuf, String)> {
    if *cancel_rx.borrow() {
        return Ok(None);
    }

    let verify_start = Instant::now();
    let cache = &*VALIDATION_CACHE;
    let cache_key = fs::metadata(&candidate.path)
        .await
        .ok()
        .map(|meta| CacheKey::from_meta(&candidate.path, &meta));

    if let Some(key) = cache_key.as_ref()
        && cache.is_valid(key, options.archive_validation)
    {
        return Ok(Some(FileRecord {
            beatmapset_id: candidate.beatmapset_id,
            file_size: key.size(),
            path: candidate.path,
            validation_error: None,
            duration_us: verify_start.elapsed().as_micros() as u64,
        }));
    }

    let mut validation_error = None;
    let mut file_size = cache_key.as_ref().map(|key| key.size()).unwrap_or(0);

    match validate_and_remove(&candidate.path, options.archive_validation).await {
        Ok(ArchiveValidationResult::Valid) => {
            if let Some(key) = cache_key {
                cache.mark_valid(key, options.archive_validation);
            }
        }
        Ok(ArchiveValidationResult::NotFound) => {
            return Ok(None);
        }
        Ok(ArchiveValidationResult::Invalid(reason)) => {
            validation_error = Some(reason);
            file_size = 0;
        }
        Err(err) => {
            return Err((candidate.path.clone(), err.to_string()));
        }
    }

    Ok(Some(FileRecord {
        beatmapset_id: candidate.beatmapset_id,
        file_size,
        path: candidate.path,
        validation_error,
        duration_us: verify_start.elapsed().as_micros() as u64,
    }))
}

#[cfg(test)]
#[path = "../../tests/unit/download_precheck.rs"]
mod tests;
