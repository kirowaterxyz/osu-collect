use super::{message_style, scroll_window, truncate_to_width};
use crate::download::BeatmapStage;
use crate::tui::{DANGER, SUCCESS, TEXT_DIM, TEXT_FAINT, WARNING};

#[test]
fn scroll_window_keeps_focus_centered() {
    let items: Vec<u8> = (0..20).collect();
    let (start, end) = scroll_window(&items, 10, 5);
    assert!((start..end).contains(&10));
    assert_eq!(end - start, 5);
}

#[test]
fn scroll_window_clamps_to_end() {
    let items: Vec<u8> = (0..10).collect();
    let (start, end) = scroll_window(&items, 9, 4);
    assert_eq!(end, 10);
    assert_eq!(start, 6);
}

#[test]
fn scroll_window_empty_visible() {
    let items: Vec<u8> = (0..10).collect();
    assert_eq!(scroll_window(&items, 3, 0), (0, 10));
}

#[test]
fn truncate_to_width_handles_zero() {
    assert_eq!(truncate_to_width("hello", 0), "");
}

#[test]
fn truncate_to_width_one_returns_ellipsis() {
    assert_eq!(truncate_to_width("hello", 1), "…");
}

#[test]
fn truncate_to_width_unicode_safe() {
    assert_eq!(truncate_to_width("こんにちは世界", 4), "こんに…");
}

#[test]
fn message_style_rate_limited_overrides() {
    use ratatui::style::Style;
    assert_eq!(
        message_style(BeatmapStage::Success, true),
        Style::default().fg(WARNING)
    );
}

#[test]
fn message_style_stage_classification() {
    use ratatui::style::Style;
    assert_eq!(
        message_style(BeatmapStage::Success, false),
        Style::default().fg(SUCCESS)
    );
    assert_eq!(
        message_style(BeatmapStage::Skipped, false),
        Style::default().fg(TEXT_FAINT)
    );
    assert_eq!(
        message_style(BeatmapStage::Failed, false),
        Style::default().fg(DANGER)
    );
    assert_eq!(
        message_style(BeatmapStage::Aborted, false),
        Style::default().fg(DANGER)
    );
    assert_eq!(
        message_style(BeatmapStage::Downloading, false),
        Style::default().fg(TEXT_DIM)
    );
    assert_eq!(
        message_style(BeatmapStage::Pending, false),
        Style::default().fg(TEXT_DIM)
    );
    assert_eq!(
        message_style(BeatmapStage::Verifying, false),
        Style::default().fg(TEXT_DIM)
    );
}
