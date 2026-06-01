use super::super::{SPINNER_FRAMES_PADDED, spinner_str};
use super::{hint_for, hint_line};
use crate::app::{App, collection::CollectionPage};
use crate::config::{Config, constants::STATIC_TABS};
use crate::download::{DownloadId, DownloadStage};

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
