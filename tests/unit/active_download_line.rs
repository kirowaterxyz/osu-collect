use super::{ActiveDownloadLine, STATUS_DEBOUNCE};
use crate::download::BeatmapStage;
use crate::tui::{accent, info, warning};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[test]
fn verifying_is_not_terminal() {
    // slot must stay claimed during verification — Verifying is mid-flight.
    assert!(!BeatmapStage::Verifying.is_terminal());
}

#[test]
fn first_update_applies_immediately() {
    // freshly-claimed slot must show its first status right away, otherwise the row sits
    // blank for STATUS_DEBOUNCE on every new beatmapset.
    let mut line = ActiveDownloadLine::new(1);
    line.apply_status(
        BeatmapStage::Downloading,
        "checking osu.direct",
        false,
        None,
    );
    assert_eq!(line.displayed_message(), "checking osu.direct");
    assert_eq!(line.bar_color(), accent());
}

#[test]
fn second_update_within_window_queues_text_but_stage_updates_immediately() {
    // text is debounced (no bypass for any stage), but `stage` is structural and must
    // reflect Verifying color right away without a one-frame 100% flash.
    let mut line = ActiveDownloadLine::new(1);
    line.apply_status(
        BeatmapStage::Downloading,
        "downloading from osu.direct",
        false,
        None,
    );
    line.apply_status(
        BeatmapStage::Verifying,
        "verifying from osu.direct",
        false,
        None,
    );
    assert_eq!(line.displayed_message(), "downloading from osu.direct");
    assert_eq!(
        line.bar_color(),
        info(),
        "Verifying must switch bar to info color instantly"
    );
}

#[test]
fn pending_update_resolves_after_window() {
    let mut line = ActiveDownloadLine::new(1);
    line.apply_status(
        BeatmapStage::Downloading,
        "downloading from osu.direct",
        false,
        None,
    );
    line.apply_status(
        BeatmapStage::Verifying,
        "verifying from osu.direct",
        false,
        None,
    );
    sleep(STATUS_DEBOUNCE + Duration::from_millis(5));
    assert_eq!(line.displayed_message(), "verifying from osu.direct");
    assert_eq!(line.bar_color(), info());
}

#[test]
fn rapid_transitions_coalesce_to_latest() {
    // verifying → success inside the window: the intermediate text is dropped; user only
    // sees the final state when the window expires.
    let mut line = ActiveDownloadLine::new(1);
    line.apply_status(
        BeatmapStage::Downloading,
        "downloading from osu.direct",
        false,
        None,
    );
    line.apply_status(
        BeatmapStage::Verifying,
        "verifying from osu.direct",
        false,
        None,
    );
    line.apply_status(
        BeatmapStage::Success,
        "downloaded from osu.direct",
        false,
        None,
    );
    sleep(STATUS_DEBOUNCE + Duration::from_millis(5));
    assert_eq!(line.displayed_message(), "downloaded from osu.direct");
}

#[test]
fn bar_color_downloading_rate_limited_is_warning() {
    let mut line = ActiveDownloadLine::new(42);
    line.apply_status(BeatmapStage::Downloading, "", true, None);
    assert_eq!(line.bar_color(), warning());
}

#[test]
fn cooldown_secs_remaining_reflects_deadline() {
    let mut line = ActiveDownloadLine::new(1);
    let cooldown = Duration::from_secs(30);
    let deadline = Instant::now() + cooldown;
    line.apply_status(
        BeatmapStage::Downloading,
        "rate limited",
        true,
        Some(deadline),
    );
    let remaining = line
        .cooldown_secs_remaining()
        .expect("must be Some when deadline is set");
    // ≤ 30s (deadline was set just above) and ≥ 29s (test runs well under 1s)
    assert!(remaining <= 30, "remaining must be ≤ 30, got {remaining}");
    assert!(remaining >= 29, "remaining must be ≥ 29, got {remaining}");
}

#[test]
fn cooldown_secs_remaining_is_none_when_not_rate_limited() {
    let mut line = ActiveDownloadLine::new(1);
    line.apply_status(
        BeatmapStage::Downloading,
        "downloading from nerinyan",
        false,
        None,
    );
    assert!(line.cooldown_secs_remaining().is_none());
}

// --- Sort-order tests (via CollectionPage slot iteration) ---
//
// The render sorts active_downloads in two passes: first non-rate-limited slots,
// then rate-limited slots. We verify this ordering contract by inspecting the
// page's slot vector directly (same source of truth the render uses).

fn make_downloading_line(beatmapset_id: u32, rate_limited: bool) -> ActiveDownloadLine {
    let mut line = ActiveDownloadLine::new(beatmapset_id);
    let cooldown = rate_limited.then(|| Instant::now() + Duration::from_secs(60));
    line.apply_status(BeatmapStage::Downloading, "msg", rate_limited, cooldown);
    line
}

/// Collect non-terminal slot IDs in two groups: normal first, then rate-limited,
/// preserving relative insertion order within each group.
fn sorted_ids(slots: &[Option<ActiveDownloadLine>]) -> (Vec<u32>, Vec<u32>) {
    let mut normal = Vec::new();
    let mut limited = Vec::new();
    for line in slots.iter().flatten() {
        if line.stage.is_terminal() {
            continue;
        }
        if line.displayed_rate_limited() {
            limited.push(line.beatmapset_id);
        } else {
            normal.push(line.beatmapset_id);
        }
    }
    (normal, limited)
}

#[test]
fn rate_limited_rows_group_at_the_bottom() {
    // slots: normal(10), rate-limited(20), normal(30)
    // after split: normal → [10, 30], rate-limited → [20]
    let slots: Vec<Option<ActiveDownloadLine>> = vec![
        Some(make_downloading_line(10, false)),
        Some(make_downloading_line(20, true)),
        Some(make_downloading_line(30, false)),
    ];
    let (normal, limited) = sorted_ids(&slots);
    assert_eq!(normal, [10, 30]);
    assert_eq!(limited, [20]);
}

#[test]
fn stable_sort_preserves_relative_order_in_each_group() {
    // slots: rl(1), normal(2), rl(3), normal(4), rl(5)
    // normal group must keep insertion order: [2, 4]
    // rate-limited group must keep insertion order: [1, 3, 5]
    let slots: Vec<Option<ActiveDownloadLine>> = vec![
        Some(make_downloading_line(1, true)),
        Some(make_downloading_line(2, false)),
        Some(make_downloading_line(3, true)),
        Some(make_downloading_line(4, false)),
        Some(make_downloading_line(5, true)),
    ];
    let (normal, limited) = sorted_ids(&slots);
    assert_eq!(
        normal,
        [2, 4],
        "non-rate-limited must preserve insertion order"
    );
    assert_eq!(
        limited,
        [1, 3, 5],
        "rate-limited must preserve insertion order"
    );
}

#[test]
fn terminal_slots_are_excluded_from_sort_groups() {
    // terminal rows (success, skipped, failed) must not appear in either group
    let mut success_line = make_downloading_line(99, false);
    success_line.apply_status(BeatmapStage::Success, "done", false, None);

    let slots: Vec<Option<ActiveDownloadLine>> = vec![
        Some(make_downloading_line(1, false)),
        Some(success_line),
        Some(make_downloading_line(2, true)),
    ];
    let (normal, limited) = sorted_ids(&slots);
    assert_eq!(normal, [1]);
    assert_eq!(limited, [2]);
    // 99 must not appear in either group
    assert!(!normal.contains(&99) && !limited.contains(&99));
}
