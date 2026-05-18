use super::super::{
    App, collection_state, failed_maps,
    messages::{clear_app_message, set_info_message, set_loading_message},
    snapshots,
    updates::{MissingBeatmapset, MissingStatus, ScanStatus, extract_collection_id},
};
use crate::{
    config::constants::CONCURRENT_REQUESTS,
    core::collection::{Collection, api_client},
    osu_db::{
        BeatmapReader, LazerReader, LocalBeatmapset, LocalCollection, OsuClient, StableReader,
    },
};
use futures_util::{StreamExt, stream};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
use tokio::sync::mpsc;
use tracing::{debug, info, trace};

type DatabaseReadResult = (Vec<LocalCollection>, Vec<LocalBeatmapset>, Vec<String>);

#[derive(Debug, Clone)]
pub enum UpdatesEvent {
    DatabaseRead {
        generation: u64,
        collections: Vec<LocalCollection>,
        beatmapsets: Vec<LocalBeatmapset>,
        all_checksums: Vec<String>,
    },
    Progress {
        generation: u64,
        message: String,
    },
    ScanComplete {
        generation: u64,
        missing: Vec<MissingBeatmapset>,
        collection_seen: HashMap<u32, Vec<u32>>,
        manually_added_count: usize,
        hidden_failed_count: usize,
    },
    FailedMapRecheckProgress {
        generation: u64,
        checked: usize,
        total: usize,
    },
    FailedMapRecheckComplete {
        generation: u64,
        available: HashSet<u32>,
        unavailable: HashSet<u32>,
    },
    Error(String),
}

pub(super) fn handle_updates_event(
    event: UpdatesEvent,
    app: &mut App,
    updates_tx: &mpsc::UnboundedSender<UpdatesEvent>,
) {
    match event {
        UpdatesEvent::DatabaseRead {
            generation,
            collections,
            beatmapsets,
            all_checksums,
        } => {
            // Ignore stale results from previous scan
            if generation != app.updates.scan.scan_generation {
                debug!(
                    expected = app.updates.scan.scan_generation,
                    got = generation,
                    "Ignoring stale DatabaseRead event"
                );
                return;
            }

            app.updates.set_collections(collections);
            app.updates.set_local_beatmapsets(beatmapsets);
            app.updates.set_all_checksums(all_checksums);
            app.updates.scan.scan_status = ScanStatus::FetchingCollection;
            set_loading_message(&mut app.updates.message, "fetching collections...");

            let selected_ids = app.updates.selected_collection_ids();
            if selected_ids.is_empty() {
                app.updates.scan.scan_status = ScanStatus::Ready;
                set_info_message(
                    &mut app.updates.message,
                    "No collections with IDs found to compare",
                );
                return;
            }

            spawn_fetch_task(app, selected_ids, updates_tx.clone());
        }
        UpdatesEvent::Progress {
            generation,
            message,
        } => {
            if generation == app.updates.scan.scan_generation {
                set_loading_message(&mut app.updates.message, message);
            }
        }
        UpdatesEvent::ScanComplete {
            generation,
            missing,
            collection_seen,
            manually_added_count,
            hidden_failed_count,
        } => {
            // Ignore stale results from previous scan
            if generation != app.updates.scan.scan_generation {
                debug!(
                    expected = app.updates.scan.scan_generation,
                    got = generation,
                    "Ignoring stale ScanComplete event"
                );
                return;
            }

            let previously_deleted_count = missing.iter().filter(|m| m.previously_deleted).count();
            let local_ids: HashSet<u32> =
                app.updates.scan.local_beatmapsets.keys().copied().collect();
            let local_snapshot: Vec<u32> = local_ids.iter().copied().collect();
            let count = missing.len();
            app.updates.set_missing_beatmaps(missing);
            app.updates.set_failed_beatmapset_count(hidden_failed_count);
            app.updates.scan.scan_status = ScanStatus::Ready;

            let msg = build_scan_summary(
                count,
                previously_deleted_count,
                manually_added_count,
                hidden_failed_count,
            );
            set_info_message(&mut app.updates.message, msg);

            for (collection_id, ids) in collection_seen {
                let installed_ids: Vec<u32> = ids
                    .iter()
                    .copied()
                    .filter(|id| local_ids.contains(id))
                    .collect();
                app.collection_state.update(
                    collection_id,
                    ids,
                    installed_ids,
                    local_snapshot.clone(),
                );
            }
            if let Some(path) = app.collection_state_path.clone() {
                let state = app.collection_state.clone();
                tokio::task::spawn_blocking(move || collection_state::save(&state, &path));
            }
        }
        UpdatesEvent::FailedMapRecheckProgress {
            generation,
            checked,
            total,
        } => {
            if generation == app.updates.scan.scan_generation {
                set_loading_message(
                    &mut app.updates.message,
                    format!("checking failed maps {checked}/{total}..."),
                );
            }
        }
        UpdatesEvent::FailedMapRecheckComplete {
            generation,
            available,
            unavailable,
        } => {
            if generation != app.updates.scan.scan_generation {
                return;
            }
            set_info_message(
                &mut app.updates.message,
                format!(
                    "{} failed maps available; {} still unavailable",
                    available.len(),
                    unavailable.len()
                ),
            );
            app.updates.scan.scan_generation = app.updates.scan.scan_generation.wrapping_add(1);
            spawn_scan_task(app, updates_tx.clone());
        }
        UpdatesEvent::Error(msg) => {
            app.updates.set_error(msg);
        }
    }
}

fn build_scan_summary(
    count: usize,
    previously_deleted: usize,
    manually_added: usize,
    hidden_failed: usize,
) -> String {
    let mut msg = format!(" {count} missing beatmapsets");
    if previously_deleted > 0 {
        msg.push_str(&format!(
            " ({previously_deleted} previously deleted — re-select to download)"
        ));
    }
    if manually_added > 0 {
        msg.push_str(&format!(
            "; {manually_added} added manually since last scan"
        ));
    }
    if hidden_failed > 0 {
        msg.push_str(&format!("; {hidden_failed} hidden failed maps"));
    }
    msg
}

pub(super) fn spawn_scan_task(app: &mut App, tx: mpsc::UnboundedSender<UpdatesEvent>) {
    if let Some(h) = app.scan_handle.take() {
        h.abort();
    }

    let client_type = app.updates.path.client_type;
    let osu_path = PathBuf::from(app.updates.osu_path());
    let generation = app.updates.scan.scan_generation;

    app.updates.scan.scan_status = ScanStatus::ReadingDatabase;
    clear_app_message(&mut app.updates.message);
    set_loading_message(&mut app.updates.message, "Reading database...");

    let handle = tokio::spawn(async move {
        let result =
            tokio::task::spawn_blocking(move || read_local_database(client_type, osu_path))
                .await
                .map_err(|e| format!("Task panicked: {e}"))
                .and_then(|r| r);

        match result {
            Ok((collections, beatmapsets, all_checksums)) => {
                let _ = tx.send(UpdatesEvent::DatabaseRead {
                    generation,
                    collections,
                    beatmapsets,
                    all_checksums,
                });
            }
            Err(err) => {
                let _ = tx.send(UpdatesEvent::Error(err));
            }
        }
    });
    app.scan_handle = Some(handle);
}

pub fn read_local_database(
    client_type: OsuClient,
    path: PathBuf,
) -> Result<DatabaseReadResult, String> {
    match client_type {
        OsuClient::Stable => {
            let reader = StableReader::new(path);
            let collections = reader.list_collections()?;
            let beatmapsets = reader.list_beatmapsets()?;
            let all_checksums = beatmapsets
                .iter()
                .flat_map(|bs| bs.beatmaps.iter().map(|b| b.checksum.clone()))
                .collect();
            Ok((collections, beatmapsets, all_checksums))
        }
        OsuClient::Lazer => {
            // Open realm once; calling list_collections/list_beatmapsets/list_all_checksums
            // individually would open the 167MB client.realm file three separate times.
            let reader = LazerReader::new(path);
            reader.read_all()
        }
    }
}

pub fn collection_ids_for_scan(selected_ids: Vec<u64>) -> Vec<u32> {
    selected_ids
        .into_iter()
        .filter_map(|id| u32::try_from(id).ok())
        .collect()
}

/// Compute `installed ∩ last_seen_remote` per collection — the set of beatmapsets the user
/// is expected to have but may have manually deleted since the last scan.
pub fn deleted_maps_for_scan(
    collection_state: &collection_state::CollectionStateFile,
    selected_collection_ids: &[u32],
) -> HashMap<u32, HashSet<u32>> {
    selected_collection_ids
        .iter()
        .map(|&id| {
            let installed: HashSet<u32> = collection_state
                .last_installed_at_scan(id)
                .iter()
                .copied()
                .collect();
            let last_seen_remote: HashSet<u32> = collection_state
                .last_seen_remote(id)
                .iter()
                .copied()
                .collect();
            (
                id,
                installed.intersection(&last_seen_remote).copied().collect(),
            )
        })
        .collect()
}

/// Compute total manually-added beatmapsets: `current_local − snapshot`.
/// Returns 0 when there is no prior snapshot for a collection (first run).
pub fn manually_added_count(
    collection_state: &collection_state::CollectionStateFile,
    selected_collection_ids: &[u32],
    local_beatmapsets: &HashMap<u32, LocalBeatmapset>,
) -> usize {
    let local_ids: HashSet<u32> = local_beatmapsets.keys().copied().collect();
    selected_collection_ids
        .iter()
        .map(|&id| {
            let snapshot = collection_state.snapshot_local_at_scan(id);
            if snapshot.is_empty() {
                return 0;
            }
            let snapshot_set: HashSet<u32> = snapshot.iter().copied().collect();
            local_ids.difference(&snapshot_set).count()
        })
        .sum()
}

pub fn snapshot_diffs_for_scan(
    snapshot_dir: &std::path::Path,
    selected_collection_ids: &[u32],
    current_snapshots: &HashMap<u32, snapshots::CollectionSnapshotFile>,
) -> HashMap<u32, snapshots::SnapshotDiff> {
    selected_collection_ids
        .iter()
        .filter_map(|&collection_id| {
            let current = current_snapshots.get(&collection_id)?;
            let path = snapshots::snapshot_path(snapshot_dir, collection_id);
            let previous = snapshots::load(&path);
            let diff = snapshots::diff_snapshot(
                previous.as_ref().map(|snapshot| &snapshot.snapshot),
                &current.snapshot,
            );
            Some((collection_id, diff))
        })
        .collect()
}

pub(super) fn spawn_failed_map_recheck_task(
    app: &mut App,
    tx: mpsc::UnboundedSender<UpdatesEvent>,
) {
    if let Some(h) = app.scan_handle.take() {
        h.abort();
    }

    let generation = app.updates.scan.scan_generation;
    let Some(path) = failed_maps::failed_maps_path() else {
        set_info_message(&mut app.updates.message, "no failed maps to check");
        return;
    };
    let ids: Vec<u32> = failed_maps::load(&path).beatmapset_ids;
    if ids.is_empty() {
        set_info_message(&mut app.updates.message, "no failed maps to check");
        return;
    }

    app.updates.scan.scan_status = ScanStatus::CheckingFailedMaps;
    set_loading_message(
        &mut app.updates.message,
        format!("checking failed maps 0/{}...", ids.len()),
    );

    let handle = tokio::spawn(async move {
        let fetcher = osu_downloader::size::SizeFetcher::new();
        let progress_tx = tx.clone();
        let mirrors = osu_downloader::Mirror::builtins();
        let result = fetcher
            .check_availability(&ids, &mirrors, |checked, total| {
                let _ = progress_tx.send(UpdatesEvent::FailedMapRecheckProgress {
                    generation,
                    checked,
                    total,
                });
            })
            .await;
        failed_maps::remove_available(&path, &result.available);
        let _ = tx.send(UpdatesEvent::FailedMapRecheckComplete {
            generation,
            available: result.available,
            unavailable: result.unavailable,
        });
    });
    app.scan_handle = Some(handle);
}

fn spawn_fetch_task(
    app: &mut App,
    selected_ids: Vec<u64>,
    tx: mpsc::UnboundedSender<UpdatesEvent>,
) {
    if let Some(h) = app.scan_handle.take() {
        h.abort();
    }

    let selected_collection_ids = collection_ids_for_scan(selected_ids);
    let local_beatmapsets: HashMap<u32, LocalBeatmapset> =
        app.updates.scan.local_beatmapsets.clone();
    let all_local_checksums = app.updates.scan.all_local_checksums.clone();
    let generation = app.updates.scan.scan_generation;
    let client_type = app.updates.path.client_type;
    let beatmapsets: Vec<LocalBeatmapset> = local_beatmapsets.values().cloned().collect();
    let current_snapshots = snapshots::current_snapshots(
        client_type,
        &app.updates.scan.local_collections_raw,
        &beatmapsets,
        |name| extract_collection_id(name).and_then(|id| u32::try_from(id).ok()),
    );
    let snapshot_dir = snapshots::snapshots_dir();
    let snapshot_diffs = snapshot_dir
        .as_deref()
        .map(|dir| snapshot_diffs_for_scan(dir, &selected_collection_ids, &current_snapshots))
        .unwrap_or_default();
    let added_count = snapshot_diffs
        .values()
        .map(|diff| diff.manually_added.len())
        .sum();
    let failed_beatmapset_ids = failed_maps::failed_maps_path()
        .as_deref()
        .map(failed_maps::load)
        .map(|failed_maps| failed_maps.ids())
        .unwrap_or_default();
    let hidden_failed_count = failed_beatmapset_ids.len();

    app.updates.scan.scan_status = ScanStatus::FetchingCollection;

    let handle = tokio::spawn(async move {
        let result = fetch_missing_beatmapsets(
            client_type,
            selected_collection_ids,
            local_beatmapsets,
            all_local_checksums,
            snapshot_diffs,
            FetchCompareSettings {
                hidden_failed_beatmapset_ids: failed_beatmapset_ids,
            },
        )
        .await;

        match result {
            Ok((missing, collection_seen)) => {
                let _ = tx.send(UpdatesEvent::ScanComplete {
                    generation,
                    missing,
                    collection_seen,
                    manually_added_count: added_count,
                    hidden_failed_count,
                });
            }
            Err(err) => {
                let _ = tx.send(UpdatesEvent::Error(err));
            }
        }
    });
    app.scan_handle = Some(handle);
}

#[derive(Debug, Clone, Default)]
pub struct FetchCompareSettings {
    pub hidden_failed_beatmapset_ids: HashSet<u32>,
}

pub fn should_hide_failed_beatmapset(settings: &FetchCompareSettings, beatmapset_id: u32) -> bool {
    settings
        .hidden_failed_beatmapset_ids
        .contains(&beatmapset_id)
}

#[derive(Debug, Clone)]
struct CollectionBeatmapset {
    id: u32,
    checksums: Vec<String>,
}

impl CollectionBeatmapset {
    fn is_in_snapshot(
        &self,
        client_type: OsuClient,
        snapshot: &snapshots::CollectionSnapshot,
    ) -> bool {
        match client_type {
            OsuClient::Stable => {
                let deleted_hashes: HashSet<&str> =
                    snapshot.stable_hashes.iter().map(String::as_str).collect();
                self.checksums.iter().any(|checksum| {
                    !checksum.is_empty() && deleted_hashes.contains(checksum.as_str())
                })
            }
            OsuClient::Lazer => snapshot.lazer_ids.contains(&u64::from(self.id)),
        }
    }
}

pub async fn fetch_missing_beatmapsets(
    client_type: OsuClient,
    collection_ids: Vec<u32>,
    local_beatmapsets: HashMap<u32, LocalBeatmapset>,
    local_checksums: HashSet<String>,
    snapshot_diffs: HashMap<u32, snapshots::SnapshotDiff>,
    settings: FetchCompareSettings,
) -> Result<(Vec<MissingBeatmapset>, HashMap<u32, Vec<u32>>), String> {
    let client = osu_downloader::collection::CollectionClient::new();
    let mut candidates_to_check: Vec<(CollectionBeatmapset, u32, String)> = Vec::new();
    let mut collection_seen: HashMap<u32, Vec<u32>> = HashMap::new();

    debug!(
        local_beatmapset_count = local_beatmapsets.len(),
        local_checksums_count = local_checksums.len(),
        "Starting fetch_and_compare"
    );

    let t_api = std::time::Instant::now();

    // Fetch all collections concurrently, then process results sequentially
    let fetched: Vec<Result<(u32, Collection), String>> = stream::iter(collection_ids)
        .map(|collection_id| {
            let client = client.clone();
            async move {
                api_client::fetch_collection(&client, collection_id)
                    .await
                    .map(|c| (collection_id, c))
                    .map_err(|e| e.to_string())
            }
        })
        .buffer_unordered(CONCURRENT_REQUESTS)
        .collect()
        .await;

    info!(
        elapsed_ms = t_api.elapsed().as_millis(),
        "phase: API fetch collections"
    );

    for fetch_result in fetched {
        let (collection_id, collection) = fetch_result?;

        debug!(
            collection_id,
            collection_name = %collection.name,
            beatmapset_count = collection.beatmapsets.len(),
            "Fetched collection from API"
        );

        let api_ids: Vec<u32> = collection.beatmapsets.iter().map(|b| b.id).collect();
        collection_seen.insert(collection_id, api_ids);

        // Dedupe within this collection only — the same beatmapset can appear
        // in multiple collections and must be tracked per collection_id.
        let mut seen_in_collection: HashSet<u32> = HashSet::new();

        for beatmapset in &collection.beatmapsets {
            if !seen_in_collection.insert(beatmapset.id) {
                continue;
            }

            // Skip if beatmapset exists locally (by ID)
            if local_beatmapsets.contains_key(&beatmapset.id) {
                trace!(beatmapset_id = beatmapset.id, "Found by ID, skipping");
                continue;
            }

            // ID not found - check if ALL checksums exist locally (handles beatmapsets with invalid OnlineID)
            let api_checksums: Vec<&str> = beatmapset
                .beatmaps
                .iter()
                .map(|bm| bm.checksum.as_str())
                .filter(|cs| !cs.is_empty())
                .collect();

            if !api_checksums.is_empty()
                && api_checksums.iter().all(|cs| local_checksums.contains(*cs))
            {
                trace!(
                    beatmapset_id = beatmapset.id,
                    "ID not found but all checksums exist locally, skipping"
                );
                continue;
            }

            if should_hide_failed_beatmapset(&settings, beatmapset.id) {
                trace!(beatmapset_id = beatmapset.id, "skipping failed beatmapset");
                continue;
            }

            trace!(
                beatmapset_id = beatmapset.id,
                "not installed, adding to candidates"
            );
            candidates_to_check.push((
                CollectionBeatmapset {
                    id: beatmapset.id,
                    checksums: beatmapset
                        .beatmaps
                        .iter()
                        .map(|beatmap| beatmap.checksum.to_string())
                        .collect(),
                },
                collection_id,
                collection.name.to_string(),
            ));
        }
    }

    debug!(
        candidates = candidates_to_check.len(),
        "finished scanning collections"
    );

    let mut all_missing: Vec<MissingBeatmapset> = Vec::new();

    for (beatmapset, collection_id, collection_name) in candidates_to_check {
        let previously_deleted = snapshot_diffs
            .get(&collection_id)
            .map(|diff| beatmapset.is_in_snapshot(client_type, &diff.manually_deleted))
            .unwrap_or(false);

        if previously_deleted {
            trace!(
                beatmapset_id = beatmapset.id,
                "marking as previously deleted"
            );
        }

        all_missing.push(MissingBeatmapset {
            id: beatmapset.id,
            status: MissingStatus::NotInstalled,
            collection_id,
            collection_name,
            selected: !previously_deleted,
            previously_deleted,
        });
    }

    all_missing.sort_by(|a, b| {
        a.collection_id
            .cmp(&b.collection_id)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok((all_missing, collection_seen))
}
