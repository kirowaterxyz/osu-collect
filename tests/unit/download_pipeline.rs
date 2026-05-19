use super::try_remove_empty_output_dir;
use crate::core::collection::{test_beatmapset, test_collection};
use crate::download::collection_db::create_selective_collection_db;
use crate::download::events::{Tally, translate_event};
use crate::download::{BeatmapStage, DownloadEvent, SelectiveDownloadCollection};
use osu_downloader::{Event as LibEvent, MirrorKind, Skip, Status};
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

fn drive_status(status: Status) -> DownloadEvent {
    let captured: std::sync::Arc<std::sync::Mutex<Option<DownloadEvent>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let captured_clone = std::sync::Arc::clone(&captured);
    let emit = move |event: DownloadEvent| {
        *captured_clone.lock().unwrap() = Some(event);
    };
    let mut tally = Tally::default();
    translate_event(
        0,
        LibEvent::BeatmapsetStatus {
            beatmapset_id: 0,
            status,
        },
        &mut tally,
        &emit,
    );
    captured.lock().unwrap().take().unwrap()
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
    let (tally, events) = drive_translate(vec![LibEvent::BeatmapsetFailed {
        beatmapset_id: 77,
        error: osu_downloader::Error::Network("timeout".into()),
        mirror: None,
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
fn already_exists_still_counts_as_skipped() {
    let (tally, _events) = drive_translate(vec![LibEvent::BeatmapsetSkipped {
        beatmapset_id: 5,
        reason: Skip::AlreadyExists,
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
fn finish_emits_summary_and_completed_stage() {
    use crate::download::events::emit_finish;
    use crate::download::{DownloadStage, DownloadSummary};

    let events: Arc<Mutex<Vec<DownloadEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);
    let emit = move |event: DownloadEvent| events_clone.lock().unwrap().push(event);

    emit_finish(
        99,
        &emit,
        DownloadSummary {
            downloaded: 3,
            skipped: 1,
            failed: 0,
            unverified: 0,
        },
    );

    let collected = events.lock().unwrap().clone();
    assert!(matches!(
        collected.as_slice(),
        [
            DownloadEvent::Finished {
                id: 99,
                summary: DownloadSummary { downloaded: 3, .. }
            },
            DownloadEvent::StageChanged {
                id: 99,
                stage: DownloadStage::Completed
            },
        ]
    ));
}

#[test]
fn duplicate_completed_events_dedupe_in_successful_set() {
    let (tally, _events) = drive_translate(vec![completed(10), completed(10)]);
    assert_eq!(tally.downloaded, 2);
    assert_eq!(tally.successful.len(), 1);
}

#[test]
fn unavailable_on_mirrors_is_recorded_as_failure() {
    let (tally, _events) = drive_translate(vec![LibEvent::BeatmapsetSkipped {
        beatmapset_id: 7,
        reason: Skip::UnavailableOnMirrors,
    }]);
    assert_eq!(tally.failed, 1);
    assert_eq!(tally.skipped, 0);
    assert!(
        tally
            .failures
            .iter()
            .any(|(id, msg)| *id == 7 && msg.contains("unavailable on all mirrors"))
    );
}

#[test]
fn completed_event_decrements_unverified_when_present() {
    let mut tally = Tally {
        unverified: 2,
        ..Tally::default()
    };
    let captured: Arc<Mutex<Vec<DownloadEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = Arc::clone(&captured);
    let emit = move |event| captured_clone.lock().unwrap().push(event);
    translate_event(1, completed(123), &mut tally, &emit);
    assert_eq!(tally.unverified, 1);
}

#[tokio::test]
async fn write_selective_collection_db_skips_empty_set() {
    use crate::download::collection_db::write_selective_collection_db;
    use std::collections::HashSet;

    let dir = tempdir().unwrap();
    let collection = test_collection(1, vec![test_beatmapset(10, &["hash"])]);
    let noop = |_event: DownloadEvent| {};

    write_selective_collection_db(
        1,
        collection,
        Vec::new(),
        HashSet::new(),
        dir.path().to_path_buf(),
        &noop,
    )
    .await
    .expect("empty verified set must succeed without writing a db");

    assert!(!dir.path().join("collection.db").exists());
}

#[test]
fn emit_status_messages_match_format_output() {
    let mirrors = [
        MirrorKind::Nerinyan,
        MirrorKind::OsuDirect,
        MirrorKind::Sayobot,
        MirrorKind::Nekoha,
    ];
    for mirror in mirrors {
        let label = mirror.label();

        let DownloadEvent::BeatmapStatus { message, .. } =
            drive_status(Status::Contacting { mirror })
        else {
            panic!("expected BeatmapStatus");
        };
        assert_eq!(message, format!("checking {label}"));

        let DownloadEvent::BeatmapStatus { message, .. } =
            drive_status(Status::Downloading { mirror })
        else {
            panic!("expected BeatmapStatus");
        };
        assert_eq!(message, format!("downloading from {label}"));

        let DownloadEvent::BeatmapStatus { message, .. } =
            drive_status(Status::Verifying { mirror })
        else {
            panic!("expected BeatmapStatus");
        };
        assert_eq!(message, format!("verifying from {label}"));

        let reasons = [
            "connection reset",
            "connection reset by peer (os error 104)",
        ];
        for reason in reasons {
            let DownloadEvent::BeatmapStatus {
                message,
                rate_limited,
                ..
            } = drive_status(Status::RetryingTransient {
                mirror,
                attempt: 2,
                max_attempts: 3,
                reason: reason.to_string(),
            })
            else {
                panic!("expected BeatmapStatus");
            };
            assert_eq!(
                message,
                format!("retrying {label} after {reason} (attempt 2/3)")
            );
            assert!(!rate_limited);
        }
    }

    let cooldowns: &[(std::time::Duration, u64)] = &[
        (std::time::Duration::from_secs(60), 60),
        (std::time::Duration::from_secs(0), 1),
        (std::time::Duration::from_secs(1), 1),
    ];
    for &(cooldown, expected_secs) in cooldowns {
        let DownloadEvent::BeatmapStatus {
            message,
            rate_limited,
            ..
        } = drive_status(Status::RateLimited { cooldown })
        else {
            panic!("expected BeatmapStatus");
        };
        assert_eq!(
            message,
            format!("rate limited on all mirrors, waiting {expected_secs}s")
        );
        assert!(rate_limited);
    }
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

    create_selective_collection_db(&collection, &selective, &newly_downloaded, dir.path()).unwrap();

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
