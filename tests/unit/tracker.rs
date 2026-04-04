use osu_collect::download::BeatmapTracker;
use std::collections::HashSet;

#[test]
fn new_tracker_all_pending() {
    let ids: HashSet<u32> = [1, 2, 3].into_iter().collect();
    let tracker = BeatmapTracker::new(ids);
    assert_eq!(tracker.pending_count(), 3);
    assert!(tracker.is_pending(1));
    assert!(tracker.is_pending(2));
    assert!(tracker.is_pending(3));
    assert!(!tracker.is_verified(1));
}

#[test]
fn mark_verified_transitions_from_pending() {
    let ids: HashSet<u32> = [1, 2].into_iter().collect();
    let tracker = BeatmapTracker::new(ids);

    assert!(tracker.mark_verified(1));
    assert!(tracker.is_verified(1));
    assert!(!tracker.is_pending(1));
    assert_eq!(tracker.pending_count(), 1);
}

#[test]
fn mark_failed_transitions_from_pending() {
    let ids: HashSet<u32> = [1].into_iter().collect();
    let tracker = BeatmapTracker::new(ids);

    assert!(tracker.mark_failed(1));
    assert!(!tracker.is_pending(1));
    assert!(!tracker.is_verified(1));
}

#[test]
fn mark_pending_only_from_failed() {
    let ids: HashSet<u32> = [1, 2].into_iter().collect();
    let tracker = BeatmapTracker::new(ids);

    assert!(!tracker.mark_pending(1));

    tracker.mark_failed(1);
    assert!(tracker.mark_pending(1));
    assert!(tracker.is_pending(1));
}

#[test]
fn remove_pending_transitions_to_in_progress() {
    let ids: HashSet<u32> = [1, 2, 3].into_iter().collect();
    let tracker = BeatmapTracker::new(ids);

    let remaining = tracker.remove_pending(1);
    assert_eq!(remaining, Some(2));
    assert!(!tracker.is_pending(1));

    assert!(tracker.remove_pending(1).is_none());
}

#[test]
fn with_verified_separates_initial_states() {
    let pending: HashSet<u32> = [1, 2].into_iter().collect();
    let verified: HashSet<u32> = [3, 4].into_iter().collect();
    let tracker = BeatmapTracker::with_verified(pending, verified);

    assert_eq!(tracker.pending_count(), 2);
    assert!(tracker.is_pending(1));
    assert!(tracker.is_verified(3));
    assert!(tracker.is_verified(4));
}

#[test]
fn is_all_complete_requires_no_pending() {
    let ids: HashSet<u32> = [1, 2].into_iter().collect();
    let tracker = BeatmapTracker::new(ids);

    assert!(!tracker.is_all_complete());

    tracker.remove_pending(1);
    tracker.mark_verified(1);
    assert!(!tracker.is_all_complete());

    tracker.remove_pending(2);
    tracker.mark_failed(2);
    assert!(tracker.is_all_complete());
}

#[test]
fn pending_snapshot_returns_only_pending() {
    let ids: HashSet<u32> = [1, 2, 3].into_iter().collect();
    let tracker = BeatmapTracker::new(ids);

    tracker.remove_pending(1);
    tracker.mark_verified(1);

    let snapshot = tracker.pending_snapshot();
    assert_eq!(snapshot.len(), 2);
    assert!(!snapshot.contains(&1));
}

#[test]
fn mark_verified_batch() {
    let ids: HashSet<u32> = [1, 2, 3, 4].into_iter().collect();
    let tracker = BeatmapTracker::new(ids);

    let count = tracker.mark_verified_batch([1, 2, 3]);
    assert_eq!(count, 3);
    assert_eq!(tracker.pending_count(), 1);
    assert!(tracker.is_pending(4));
}

#[test]
fn validation_cache_hit_and_miss() {
    let tracker = BeatmapTracker::default();
    let path = std::path::PathBuf::from("/tmp/test.osz");

    assert!(tracker.check_validation_cache(&path, 1024, None).is_none());

    tracker.cache_validation_result(path.clone(), 1024, None, true);
    assert_eq!(
        tracker.check_validation_cache(&path, 1024, None),
        Some(true)
    );

    tracker.invalidate_cache(&path);
    assert!(tracker.check_validation_cache(&path, 1024, None).is_none());
}

#[test]
fn mark_failed_unknown_id_returns_false() {
    let ids: HashSet<u32> = [1].into_iter().collect();
    let tracker = BeatmapTracker::new(ids);
    assert!(!tracker.mark_failed(999));
}
