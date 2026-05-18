use osu_collect::app::collection::{ActiveDownloadLine, STATUS_DEBOUNCE};
use osu_collect::download::BeatmapStage;
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
    assert!(line.should_show_bar());
}

#[test]
fn second_update_within_window_queues_text_but_stage_updates_immediately() {
    // text is debounced (no bypass for any stage), but `stage` is structural and must
    // reflect Verifying right away so the bar hides without a one-frame 100% flash.
    let mut line = ActiveDownloadLine::new(1);
    line.apply_status(
        BeatmapStage::Downloading,
        "downloading from osu.direct",
        false,
    );
    line.apply_status(BeatmapStage::Verifying, "verifying from osu.direct", false);
    assert_eq!(line.displayed_message(), "downloading from osu.direct");
    assert!(
        !line.should_show_bar(),
        "Verifying must hide the bar instantly"
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
    assert!(!line.should_show_bar());
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
