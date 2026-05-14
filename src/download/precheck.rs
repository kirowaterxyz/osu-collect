use super::{
    BeatmapStage, DownloadError, DownloadEvent, DownloadId, ShutdownToken,
    integrity::ExpectationIndex,
};
use crate::worker::{
    StatusSink,
    io::{ArchiveValidationOptions, ArchiveValidationResult, validate_archive},
};
use futures_util::{StreamExt, stream};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::fs;
use tracing::{debug, info, warn};

pub(crate) struct PrecheckReport {
    pub(crate) satisfied: HashSet<u32>,
    pub(crate) skipped: u32,
    pub(crate) unverified: Vec<u32>,
    pub(crate) verified_bytes: u64,
    pub(crate) aborted: bool,
    pub(crate) files_changed: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PrecheckOptions {
    pub(crate) verify_integrity: bool,
    pub(crate) notify_verified: bool,
    pub(crate) verify_zip_eocd: bool,
}

pub(crate) async fn verify_existing_beatmapsets(
    id: DownloadId,
    output_dir: &Path,
    expectations: Arc<ExpectationIndex>,
    parallelism: usize,
    options: PrecheckOptions,
    shutdown: &ShutdownToken,
    status: &StatusSink,
) -> Result<PrecheckReport, DownloadError> {
    info!(
        download_id = id,
        directory = %output_dir.display(),
        notify_verified = options.notify_verified,
        verify_integrity = options.verify_integrity,
        verify_zip_eocd = options.verify_zip_eocd,
        "Starting existing file verification"
    );

    if shutdown.is_cancelled() {
        return Ok(PrecheckReport {
            satisfied: HashSet::new(),
            skipped: 0,
            unverified: Vec::new(),
            verified_bytes: 0,
            aborted: true,
            files_changed: false,
        });
    }

    // Capture initial snapshot for change detection
    let initial_snapshot = capture_osz_snapshot(output_dir).await?;

    let mut satisfied = HashSet::new();
    let mut skipped: u32 = 0;
    let mut verified_bytes: u64 = 0;
    let mut satisfied_sizes: HashMap<u32, u64> = HashMap::new();
    let mut unverified_maps: HashSet<u32> = HashSet::new();

    let expectation_data = expectations.data();

    let mut dir = fs::read_dir(output_dir).await?;

    let worker_count = parallelism.max(1);

    #[derive(Debug)]
    struct FileRecord {
        beatmapset_id: u32,
        file_size: u64,
        path: PathBuf,
        validation_error: Option<String>,
        duration_us: u64,
    }

    let mut candidates = Vec::new();
    let mut orphan_temp_count: usize = 0;
    while let Some(entry) = dir.next_entry().await? {
        if shutdown.is_cancelled() {
            return Ok(PrecheckReport {
                satisfied,
                skipped,
                unverified: Vec::new(),
                verified_bytes,
                aborted: true,
                files_changed: false,
            });
        }

        let file_name = entry.file_name();
        if is_orphan_temp_name(&file_name) {
            let path = entry.path();
            match fs::remove_file(&path).await {
                Ok(()) => {
                    orphan_temp_count = orphan_temp_count.saturating_add(1);
                    debug!(file = %path.display(), "Removed orphaned temp download file");
                }
                Err(err) => {
                    warn!(file = %path.display(), error = %err, "Failed to remove orphaned temp download file");
                }
            }
            continue;
        }
        if !is_osz_name(&file_name) {
            continue;
        }

        let path = entry.path();

        let Some(beatmapset_id) = extract_beatmapset_id_from_filename(&path) else {
            debug!(file = %path.display(), "Could not extract beatmapset ID from filename");
            continue;
        };

        if !expectation_data.by_set.contains(&beatmapset_id) {
            continue;
        }

        candidates.push((path, beatmapset_id));
    }

    let mut tasks = stream::iter(candidates)
        .map(|(path, beatmapset_id)| {
            let opts = ArchiveValidationOptions {
                verify_zip_eocd: options.verify_zip_eocd,
                remove_on_invalid: true,
            };
            let shutdown_inner = shutdown.clone();
            async move {
                if shutdown_inner.is_cancelled() {
                    return Ok(None);
                }

                let verify_start = Instant::now();
                let mut validation_error = None;
                let mut file_size = 0u64;

                match validate_archive(&path, opts).await {
                    Ok(ArchiveValidationResult::Valid) => {
                        if let Ok(meta) = fs::metadata(&path).await {
                            file_size = meta.len();
                        }
                    }
                    Ok(ArchiveValidationResult::NotFound) => {
                        return Ok(None);
                    }
                    Ok(ArchiveValidationResult::Invalid(reason))
                    | Ok(ArchiveValidationResult::Removed(reason)) => {
                        validation_error = Some(reason);
                    }
                    Err(e) => {
                        return Err((path.clone(), e.to_string()));
                    }
                }

                let duration_us = verify_start.elapsed().as_micros() as u64;
                Ok(Some(FileRecord {
                    beatmapset_id,
                    file_size,
                    path,
                    validation_error,
                    duration_us,
                }))
            }
        })
        .buffer_unordered(worker_count);

    let mut unverified_list = Vec::new();
    let mut aborted = false;

    while let Some(result) = tasks.next().await {
        if shutdown.is_cancelled() {
            aborted = true;
            break;
        }

        match result {
            Ok(Some(record)) => {
                if let Some(error) = record.validation_error {
                    if unverified_maps.insert(record.beatmapset_id) {
                        unverified_list.push(record.beatmapset_id);
                    }
                    warn!(
                        download_id = id,
                        beatmapset_id = record.beatmapset_id,
                        file = %record.path.display(),
                        error = %error,
                        "Existing archive failed validation"
                    );
                    if options.notify_verified {
                        status.emit(DownloadEvent::BeatmapStatus {
                            id,
                            beatmapset_id: record.beatmapset_id,
                            stage: BeatmapStage::Failed,
                            message: format!("Existing file failed validation: {}", error),
                        });
                        status.emit(DownloadEvent::BeatmapVerified {
                            id,
                            duration_us: record.duration_us,
                        });
                        status.emit(DownloadEvent::OverallProgress {
                            id,
                            downloaded: 0,
                            skipped,
                            failed: 0,
                            unverified: unverified_list.len() as u32,
                        });
                    }
                    continue;
                }

                if satisfied.insert(record.beatmapset_id) {
                    skipped = skipped.saturating_add(1);
                    verified_bytes += record.file_size;
                    satisfied_sizes.insert(record.beatmapset_id, record.file_size);
                    if options.notify_verified {
                        status.emit(DownloadEvent::BeatmapStatus {
                            id,
                            beatmapset_id: record.beatmapset_id,
                            stage: BeatmapStage::Skipped,
                            message: "Already present".to_string(),
                        });
                        status.emit(DownloadEvent::BeatmapVerified {
                            id,
                            duration_us: record.duration_us,
                        });
                        status.emit(DownloadEvent::OverallProgress {
                            id,
                            downloaded: 0,
                            skipped,
                            failed: 0,
                            unverified: unverified_list.len() as u32,
                        });
                    }
                }
            }
            Ok(None) => {}
            Err((path, error)) => {
                warn!(file = %path.display(), error = %error, "Failed to process existing file");
            }
        }
    }

    if aborted {
        info!(
            download_id = id,
            verified = satisfied.len(),
            "Existing file verification aborted by shutdown"
        );
    } else {
        info!(
            download_id = id,
            verified = satisfied.len(),
            skipped,
            unverified = unverified_list.len(),
            orphan_temp = orphan_temp_count,
            "Existing file verification complete"
        );
        if orphan_temp_count > 0 {
            status.emit(DownloadEvent::Log {
                id,
                message: format!(
                    "removed {} orphaned temp download file(s)",
                    orphan_temp_count
                ),
            });
        }
    }

    // Check if files changed during precheck
    let mut changed_ids: HashSet<u32> = HashSet::new();
    let files_changed = match capture_osz_snapshot(output_dir).await {
        Ok(final_snapshot) => {
            let changed = final_snapshot != initial_snapshot;
            if changed {
                changed_ids = detect_changed_beatmapsets(&initial_snapshot, &final_snapshot);
                info!(
                    download_id = id,
                    initial = initial_snapshot.len(),
                    final_count = final_snapshot.len(),
                    changed = changed_ids.len(),
                    "Files changed during precheck"
                );
            }
            changed
        }
        Err(err) => {
            warn!(
                download_id = id,
                error = %err,
                "Failed to capture final snapshot after precheck"
            );
            false
        }
    };

    if !changed_ids.is_empty() {
        info!(
            download_id = id,
            changed = changed_ids.len(),
            "Revalidating beatmapsets altered during precheck"
        );
        for beatmapset_id in changed_ids {
            if satisfied.remove(&beatmapset_id) {
                skipped = skipped.saturating_sub(1);
                if let Some(size) = satisfied_sizes.remove(&beatmapset_id) {
                    verified_bytes = verified_bytes.saturating_sub(size);
                }
            }
            if unverified_maps.insert(beatmapset_id) {
                unverified_list.push(beatmapset_id);
            }
            if options.notify_verified {
                status.emit(DownloadEvent::BeatmapStatus {
                    id,
                    beatmapset_id,
                    stage: BeatmapStage::Pending,
                    message: "File changed during precheck; re-downloading".to_string(),
                });
            }
        }
    }

    Ok(PrecheckReport {
        satisfied,
        skipped,
        unverified: unverified_list,
        verified_bytes,
        aborted,
        files_changed,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OszSnapshotEntry {
    name: Box<str>,
    size: u64,
    modified_micros: Option<u128>,
}

async fn capture_osz_snapshot(dir: &Path) -> Result<Vec<OszSnapshotEntry>, DownloadError> {
    let mut snapshot = Vec::new();
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        if !is_osz_name(&file_name) {
            continue;
        }

        let metadata = entry.metadata().await?;
        let file_name = file_name.to_string_lossy().into_owned().into_boxed_str();
        let modified_micros = metadata.modified().ok().and_then(system_time_to_micros);

        snapshot.push(OszSnapshotEntry {
            name: file_name,
            size: metadata.len(),
            modified_micros,
        });
    }

    snapshot.sort();
    Ok(snapshot)
}

fn system_time_to_micros(time: SystemTime) -> Option<u128> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_micros())
}

fn detect_changed_beatmapsets(
    initial: &[OszSnapshotEntry],
    final_snapshot: &[OszSnapshotEntry],
) -> HashSet<u32> {
    let initial_map: HashMap<&str, &OszSnapshotEntry> = initial
        .iter()
        .map(|entry| (entry.name.as_ref(), entry))
        .collect();
    let final_map: HashMap<&str, &OszSnapshotEntry> = final_snapshot
        .iter()
        .map(|entry| (entry.name.as_ref(), entry))
        .collect();

    let mut changes = HashSet::new();

    for (name, previous) in &initial_map {
        match final_map.get(name) {
            Some(current) => {
                if (previous.size != current.size
                    || previous.modified_micros != current.modified_micros)
                    && let Some(id) = extract_beatmapset_id_from_filename(Path::new(name))
                {
                    changes.insert(id);
                }
            }
            None => {
                if let Some(id) = extract_beatmapset_id_from_filename(Path::new(name)) {
                    changes.insert(id);
                }
            }
        }
    }

    for name in final_map.keys() {
        if !initial_map.contains_key(name)
            && let Some(id) = extract_beatmapset_id_from_filename(Path::new(name))
        {
            changes.insert(id);
        }
    }

    changes
}

#[inline]
fn is_osz_file(path: &Path) -> bool {
    path.extension().is_some_and(is_osz_extension)
}

#[inline]
fn is_osz_name(name: &OsStr) -> bool {
    is_osz_file(Path::new(name))
}

#[inline]
fn is_osz_extension(ext: &OsStr) -> bool {
    ext.eq_ignore_ascii_case("osz")
}

#[inline]
fn is_orphan_temp_name(name: &OsStr) -> bool {
    name.to_str()
        .map(|s| {
            // matches temp files produced by `temp_path_for` in worker::io:
            // `<original_name>.part-<pid>-<counter>`
            if let Some(idx) = s.find(".part-") {
                let tail = &s[idx + ".part-".len()..];
                let mut parts = tail.splitn(2, '-');
                let pid = parts.next().unwrap_or("");
                let counter = parts.next().unwrap_or("");
                !pid.is_empty()
                    && !counter.is_empty()
                    && pid.bytes().all(|b| b.is_ascii_digit())
                    && counter.bytes().all(|b| b.is_ascii_digit())
            } else {
                false
            }
        })
        .unwrap_or(false)
}

#[inline]
pub(crate) fn extract_beatmapset_id_from_filename(path: &Path) -> Option<u32> {
    let filename = path.file_stem()?.to_str()?;
    let mut chars = filename.chars().peekable();
    let mut id = String::new();

    while let Some(ch) = chars.next_if(|ch| ch.is_ascii_digit()) {
        id.push(ch);
    }

    if id.is_empty() {
        return None;
    }

    match chars.peek() {
        None | Some(' ') => id.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_orphan_temp_files() {
        let yes = [
            "123.osz.part-12345-0",
            "abc.osz.part-1-9",
            "1 artist.osz.part-99999-42",
        ];
        let no = [
            "123.osz",
            "123.osz.part",
            "123.osz.part-abc-9",
            "123.osz.part-9-abc",
            "123.osz.part-9",
            "random.txt",
        ];
        for name in yes {
            assert!(
                is_orphan_temp_name(OsStr::new(name)),
                "expected match: {}",
                name
            );
        }
        for name in no {
            assert!(
                !is_orphan_temp_name(OsStr::new(name)),
                "expected no match: {}",
                name
            );
        }
    }

    #[test]
    fn extracts_exact_prefixed_beatmapset_ids() {
        assert_eq!(
            extract_beatmapset_id_from_filename(Path::new("123.osz")),
            Some(123)
        );
        assert_eq!(
            extract_beatmapset_id_from_filename(Path::new("123 artist.osz")),
            Some(123)
        );
        assert_eq!(
            extract_beatmapset_id_from_filename(Path::new("1234.osz")),
            Some(1234)
        );
        assert_eq!(
            extract_beatmapset_id_from_filename(Path::new("123abc.osz")),
            None
        );
    }
}
