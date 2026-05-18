use osu_collect::{
    core::collection::model::{test_beatmapset, test_collection},
    download::{
        BeatmapStage, DownloadEvent, SelectiveDownloadCollection,
        pipeline::{
            Tally, create_selective_collection_database, translate_event,
            try_remove_empty_output_dir,
        },
    },
};
use osu_downloader::{DownloadEvent as LibEvent, MirrorKind, SkipReason};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use tempfile::tempdir;

fn make_selective(id: u32, name: &str, beatmapset_ids: Vec<u32>) -> SelectiveDownloadCollection {
    SelectiveDownloadCollection {
        id,
        name: name.to_string(),
        beatmapset_ids,
    }
}

fn drive_translate(events: Vec<LibEvent>) -> (Tally, Vec<DownloadEvent>) {
    let captured: Arc<Mutex<Vec<DownloadEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = Arc::clone(&captured);
    let emit = move |event: DownloadEvent| captured_clone.lock().unwrap().push(event);
    let mut tally = Tally::default();
    for event in events {
        translate_event(42, event, &mut tally, &emit);
    }
    let collected = std::mem::take(&mut *captured.lock().unwrap());
    (tally, collected)
}

fn last_overall_progress(events: &[DownloadEvent]) -> &DownloadEvent {
    events
        .iter()
        .rev()
        .find(|event| matches!(event, DownloadEvent::OverallProgress { .. }))
        .expect("at least one OverallProgress emission")
}

fn completed(beatmapset_id: u32) -> LibEvent {
    LibEvent::BeatmapsetCompleted {
        beatmapset_id,
        filename: format!("{beatmapset_id}.osz"),
        size_bytes: 0,
        md5_hash: Some("md5".into()),
        mirror_used: MirrorKind::Nerinyan,
        verify_duration_us: 0,
    }
}

#[test]
fn completed_events_populate_tally_successful() {
    let (tally, _events) = drive_translate(vec![completed(10), completed(20)]);
    assert_eq!(tally.downloaded, 2);
    assert!(tally.successful.contains(&10) && tally.successful.contains(&20));
    assert_eq!(tally.to_summary().downloaded, 2);
}

#[test]
fn missing_progress_total_translates_to_zero_total() {
    let (_tally, events) = drive_translate(vec![LibEvent::Progress {
        beatmapset_id: 42,
        downloaded_bytes: 1_500_000,
        total_bytes: None,
        speed_bps: 0,
    }]);

    assert!(matches!(
        events.as_slice(),
        [DownloadEvent::BeatmapProgress {
            id: 42,
            beatmapset_id: 42,
            downloaded: 1_500_000,
            total: 0,
        }]
    ));
}

#[test]
fn network_error_counts_as_failed() {
    let (tally, events) = drive_translate(vec![LibEvent::BeatmapsetNetworkError {
        beatmapset_id: 77,
        reason: "timeout".into(),
    }]);
    assert_eq!(tally.failed, 1);
    assert!(tally.failures.iter().any(|(id, _)| *id == 77));
    assert!(events.iter().any(|event| matches!(
        event,
        DownloadEvent::BeatmapStatus {
            beatmapset_id: 77,
            stage: BeatmapStage::Failed,
            ..
        }
    )));
    let DownloadEvent::OverallProgress { failed, .. } = last_overall_progress(&events) else {
        unreachable!()
    };
    assert_eq!(*failed, 1);
}

#[test]
fn unavailable_on_mirrors_counts_as_failed_not_skipped() {
    let (tally, _events) = drive_translate(vec![LibEvent::BeatmapsetSkipped {
        beatmapset_id: 5,
        reason: SkipReason::UnavailableOnMirrors,
    }]);
    assert_eq!(tally.failed, 1);
    assert_eq!(tally.skipped, 0);
}

#[test]
fn already_exists_still_counts_as_skipped() {
    let (tally, _events) = drive_translate(vec![LibEvent::BeatmapsetSkipped {
        beatmapset_id: 5,
        reason: SkipReason::AlreadyExists,
    }]);
    assert_eq!(tally.skipped, 1);
    assert_eq!(tally.failed, 0);
}

#[tokio::test]
async fn empty_output_dir_is_removed_after_cancel() {
    let root = tempdir().unwrap();
    let empty = root.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let occupied = root.path().join("occupied");
    std::fs::create_dir_all(&occupied).unwrap();
    std::fs::write(occupied.join("123.osz"), b"hi").unwrap();

    let noop = |_event: DownloadEvent| {};

    try_remove_empty_output_dir(7, &empty, &noop).await;
    assert!(!empty.exists(), "empty output dir must be removed");

    try_remove_empty_output_dir(7, &occupied, &noop).await;
    assert!(occupied.exists(), "non-empty output dir must remain");
}

#[test]
fn only_newly_downloaded_hashes_are_included() {
    let dir = tempdir().unwrap();
    let collection = test_collection(
        1,
        vec![
            test_beatmapset(10, &["hash-a1", "hash-a2"]),
            test_beatmapset(20, &["hash-b1"]),
            test_beatmapset(30, &["hash-c1"]),
        ],
    );
    let selective = vec![make_selective(1, "my collection", vec![10, 20, 30])];
    let newly_downloaded: HashSet<u32> = [10].into_iter().collect();

    create_selective_collection_database(&collection, &selective, &newly_downloaded, dir.path())
        .unwrap();

    let list =
        osu_db::collection::CollectionList::from_file(dir.path().join("collection.db")).unwrap();
    assert_eq!(list.collections.len(), 1);
    let hashes: Vec<_> = list.collections[0]
        .beatmap_hashes
        .iter()
        .flatten()
        .collect();
    assert_eq!(hashes.len(), 2);
}
