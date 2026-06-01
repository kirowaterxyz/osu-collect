use super::super::{SPINNER_FRAMES_PADDED, spinner_str};
use super::{hint_for, hint_line};
use crate::app::{App, HomeField, collection::CollectionPage, collection::FailureReason};
use crate::config::{Config, constants::STATIC_TABS};
use crate::download::{DownloadId, DownloadStage, FailedMap};

#[test]
fn spinner_wraps_correctly() {
    for tick in 0u64..30 {
        let frame = spinner_str(tick);
        assert!(SPINNER_FRAMES_PADDED.contains(&frame));
    }
}

#[test]
fn hint_line_has_key_and_label_spans() {
    let line = hint_line("↑↓ move  ·  q quit");
    let full: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(full.contains("↑↓"));
    assert!(full.contains("move"));
    assert!(full.contains("q"));
    assert!(full.contains("quit"));
}

fn push_focused_page(app: &mut App, id: DownloadId, stage: DownloadStage) {
    let mut page = CollectionPage::new(id, format!("col {id}"), 1);
    page.stage = stage;
    app.downloads.push(page);
    app.active_tab = STATIC_TABS + app.downloads.len() - 1;
}

#[test]
fn footer_hint_includes_close_on_completed_tab() {
    let mut app = App::new(Config::default());
    push_focused_page(&mut app, 1, DownloadStage::Completed);

    let hint = hint_for(&app);
    assert!(
        hint.contains("close"),
        "completed-tab hint should advertise `close`, got: {hint}"
    );
}

#[test]
fn footer_hint_includes_close_on_failed_tab() {
    let mut app = App::new(Config::default());
    push_focused_page(&mut app, 1, DownloadStage::Failed);

    let hint = hint_for(&app);
    assert!(
        hint.contains("close"),
        "failed-tab hint must include `close`, got: {hint}"
    );
}

#[test]
fn footer_hint_omits_close_on_downloading_tab() {
    let mut app = App::new(Config::default());
    push_focused_page(&mut app, 1, DownloadStage::Downloading);

    let hint = hint_for(&app);
    assert!(
        !hint.contains("close"),
        "in-progress hint must not advertise close: {hint}"
    );
    assert!(
        hint.contains("q abort"),
        "in-progress hint must keep abort: {hint}"
    );
}

#[test]
fn footer_hint_caps_at_four_segments_on_settled_tab() {
    let mut app = App::new(Config::default());
    push_focused_page(&mut app, 1, DownloadStage::Completed);

    let hint = hint_for(&app);
    let segments = hint.split('·').count();
    assert!(
        segments <= 4,
        "footer must keep <=4 hints, got {segments}: {hint}"
    );
}

#[test]
fn footer_hint_includes_retry_when_page_has_failed_maps() {
    let mut app = App::new(Config::default());
    push_focused_page(&mut app, 1, DownloadStage::Failed);
    app.downloads[0].failed_maps.push(FailedMap {
        beatmapset_id: 1,
        title: None,
        reason: FailureReason::NotFound,
    });

    let hint = hint_for(&app);
    assert!(
        hint.contains("r retry"),
        "failed-maps hint must advertise `r retry`, got: {hint}"
    );
    assert!(
        hint.contains("R retry all"),
        "failed-maps hint must advertise `R retry all`, got: {hint}"
    );
}

#[test]
fn footer_hint_omits_retry_without_failed_maps() {
    let mut app = App::new(Config::default());
    push_focused_page(&mut app, 1, DownloadStage::Failed);

    let hint = hint_for(&app);
    assert!(
        !hint.contains("retry"),
        "hint without failed maps must not advertise retry: {hint}"
    );
}

#[test]
fn home_hint_shows_quit_on_non_text_input_row() {
    let mut app = App::new(Config::default());
    app.home.focus = HomeField::AutoOverwrite;

    let hint = hint_for(&app);
    assert!(
        hint.contains("q quit"),
        "non-text-input home row must advertise `q quit`, got: {hint}"
    );
}

#[test]
fn home_hint_shows_esc_quit_on_text_input_row() {
    let mut app = App::new(Config::default());
    app.home.focus = HomeField::Collection;

    let hint = hint_for(&app);
    assert!(
        hint.contains("esc quit"),
        "text-input home row must advertise `esc quit`, got: {hint}"
    );
    assert!(
        !hint.contains("q quit"),
        "text-input home row must not show `q quit` (q types into field): {hint}"
    );
}
