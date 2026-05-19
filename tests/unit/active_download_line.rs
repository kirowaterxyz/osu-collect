use super::{ActiveDownloadLine, STATUS_DEBOUNCE};
use crate::download::BeatmapStage;
use crate::tui::{ACCENT, INFO, WARNING};
use std::thread::sleep;
use std::time::Duration;

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
    line.apply_status(BeatmapStage::Downloading, "checking osu.direct", false);
    assert_eq!(line.displayed_message(), "checking osu.direct");
    assert_eq!(line.bar_color(), ACCENT);
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
    );
    line.apply_status(BeatmapStage::Verifying, "verifying from osu.direct", false);
    assert_eq!(line.displayed_message(), "downloading from osu.direct");
    assert_eq!(
        line.bar_color(),
        INFO,
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
    );
    line.apply_status(BeatmapStage::Verifying, "verifying from osu.direct", false);
    sleep(STATUS_DEBOUNCE + Duration::from_millis(5));
    assert_eq!(line.displayed_message(), "verifying from osu.direct");
    assert_eq!(line.bar_color(), INFO);
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
    );
    line.apply_status(BeatmapStage::Verifying, "verifying from osu.direct", false);
    line.apply_status(BeatmapStage::Success, "downloaded from osu.direct", false);
    sleep(STATUS_DEBOUNCE + Duration::from_millis(5));
    assert_eq!(line.displayed_message(), "downloaded from osu.direct");
}

#[test]
fn bar_color_downloading_rate_limited_is_warning() {
    let mut line = ActiveDownloadLine::new(42);
    line.apply_status(BeatmapStage::Downloading, "", true);
    assert_eq!(line.bar_color(), WARNING);
}
